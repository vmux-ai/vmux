use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use vmux_ecs::{JobWake, Operation, Revision};
use vmux_feature::http::Credential;

use crate::availability::SecureCredentials;
use crate::catalog::Catalog;
pub use crate::cloud::transport::LocalCertificate as LocalTrust;
use crate::cloud::transport::{LocalCertificate, NativePeer, Reply};
use crate::connection::{
    DeviceKey, OpenRequest, PollReady, PollRequest, Rejected, SendRequest, TransportReady,
};
use crate::wire::{Request, Response};
use crate::{Account, Binding, Location, Root, Server};

pub struct ControlBrokerPlugin;

impl bevy_app::Plugin for ControlBrokerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StoredKeyRequest>().add_systems(
            Update,
            (
                retire, configure, stored, open, handshake, receive, refresh, fail,
            )
                .chain()
                .after(crate::Set::Publish),
        );
    }
}

#[derive(Component)]
#[component(immutable)]
#[require(Revision)]
pub struct ControlBroker {
    pub environment: String,
}

#[derive(Component)]
#[require(Step = Step::Key)]
struct Peer;

#[derive(Component)]
#[component(immutable)]
struct Context {
    account: Entity,
    local: Uuid,
    remote: Uuid,
    generation: u64,
    server: Server,
    data: std::path::PathBuf,
    environment: String,
    certificate: Option<LocalCertificate>,
}

#[derive(Component)]
enum Step {
    Key,
    Open,
    Challenge,
    Prove,
    Lease,
    Wait,
    Retry,
}

#[derive(Component)]
struct ProvenDevice(Uuid);

#[derive(Component)]
struct NextAt(Instant);

#[derive(Component)]
pub struct Unavailable(pub crate::Failure);

#[derive(Message)]
pub(crate) struct StoredKeyRequest {
    pub entity: Entity,
    #[cfg(target_os = "macos")]
    pub data: std::path::PathBuf,
    #[cfg(target_os = "macos")]
    pub scope: String,
}

#[derive(Component)]
pub(crate) struct StoredKeyPrepared(pub Option<Result<Option<DeviceKey>, String>>);

#[derive(bevy_ecs::system::SystemParam)]
struct Current<'w, 's> {
    catalog: Catalog<'w, 's>,
    roots: ConfiguredRoots<'w, 's>,
    accounts: Query<'w, 's, &'static Binding, With<Account>>,
}

type ConfiguredRoots<'w, 's> = Query<
    'w,
    's,
    (
        &'static Server,
        &'static Location,
        Option<&'static LocalCertificate>,
    ),
    (With<Root>, With<SecureCredentials>),
>;

impl Current<'_, '_> {
    fn context(&self, root: Entity, environment: &str) -> Option<Context> {
        let (account, local) = self.catalog.account(root)?;
        let (server, location, certificate) = self.roots.get(root).ok()?;
        let binding = self.accounts.get(account).ok()?;
        if !binding.is_valid() || binding.server != server.authority() {
            return None;
        }
        let remote = Uuid::parse_str(binding.account_id.as_deref()?).ok()?;
        Some(Context {
            account,
            local,
            remote,
            generation: binding.credential_generation,
            server: server.clone(),
            data: location.0.clone(),
            environment: environment.into(),
            certificate: certificate.cloned(),
        })
    }
}

impl Context {
    fn matches(&self, other: &Self) -> bool {
        self.account == other.account
            && self.local == other.local
            && self.remote == other.remote
            && self.generation == other.generation
            && self.server == other.server
            && self.data == other.data
            && self.environment == other.environment
            && self.certificate == other.certificate
    }
}

fn retire(
    brokers: Query<(Entity, &ChildOf, &ControlBroker)>,
    peers: Query<(Entity, &ChildOf, &Context), With<Peer>>,
    current: Current,
    mut commands: Commands,
) {
    for (entity, owner, context) in &peers {
        let valid = brokers
            .get(owner.parent())
            .ok()
            .and_then(|(_, root, broker)| current.context(root.parent(), &broker.environment))
            .is_some_and(|now| context.matches(&now));
        if valid {
            continue;
        }
        commands.entity(entity).despawn();
        if let Ok(mut broker) = commands.get_entity(owner.parent()) {
            broker
                .remove::<Credential>()
                .remove::<NextAt>()
                .insert(Unavailable(crate::Failure::not_authenticated()));
        }
    }
}

fn configure(
    brokers: Query<(Entity, &ChildOf, &ControlBroker, Option<&NextAt>)>,
    peers: Query<&ChildOf, With<Peer>>,
    current: Current,
    mut requests: MessageWriter<StoredKeyRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (broker, root, declaration, next) in &brokers {
        if next.is_some_and(|next| next.0 > Instant::now())
            || peers.iter().any(|owner| owner.parent() == broker)
        {
            continue;
        }
        let Some(context) = current.context(root.parent(), &declaration.environment) else {
            continue;
        };
        #[cfg(target_os = "macos")]
        let scope = format!(
            "{}/{}/{}",
            context.local,
            context.server.authority(),
            context.generation
        );
        #[cfg(target_os = "macos")]
        let data = context.data.clone();
        let certificate = context.certificate.clone();
        let entity = commands
            .spawn((
                Operation::bundle("account.control").expect("static operation kind"),
                Peer,
                NativePeer,
                Step::Key,
                context,
                ChildOf(broker),
            ))
            .id();
        requests.write(StoredKeyRequest {
            entity,
            #[cfg(target_os = "macos")]
            data,
            #[cfg(target_os = "macos")]
            scope,
        });
        if let Some(certificate) = certificate {
            commands.entity(entity).insert(certificate);
        }
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn stored(
    mut prepared: Query<(Entity, &Step, &mut StoredKeyPrepared), With<Peer>>,
    mut opens: MessageWriter<OpenRequest>,
    contexts: Query<&Context>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, step, mut prepared) in &mut prepared {
        if !matches!(step, Step::Key) {
            continue;
        }
        let Some(result) = prepared.0.take() else {
            continue;
        };
        let key = match result {
            Ok(Some(key)) => key,
            Ok(None) => {
                commands
                    .entity(entity)
                    .insert(Rejected(crate::Failure::not_authenticated()));
                continue;
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(crate::Failure::storage(error)));
                continue;
            }
        };
        let Ok(context) = contexts.get(entity) else {
            continue;
        };
        commands
            .entity(entity)
            .insert((key, Step::Open))
            .remove::<StoredKeyPrepared>();
        opens.write(OpenRequest {
            entity,
            server: context.server.clone(),
        });
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

type OpenedPeers<'w, 's> =
    Query<'w, 's, (Entity, &'static Step, &'static DeviceKey), (With<Peer>, With<TransportReady>)>;

fn open(
    peers: OpenedPeers,
    mut requests: MessageWriter<SendRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, step, key) in &peers {
        if !matches!(step, Step::Open) {
            continue;
        }
        requests.write(SendRequest {
            entity,
            request: Request::Challenge {
                device_id: key.id.to_string(),
            },
        });
        commands
            .entity(entity)
            .insert(Step::Challenge)
            .remove::<TransportReady>();
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn handshake(
    mut peers: Query<(Entity, &Context, &Step, &DeviceKey, &mut Reply), With<Peer>>,
    mut requests: MessageWriter<SendRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, context, step, key, mut reply) in &mut peers {
        if !matches!(step, Step::Challenge | Step::Prove) {
            continue;
        }
        let Some(response) = reply.0.take() else {
            continue;
        };
        match (step, response) {
            (Step::Challenge, Ok(Response::Challenge { nonce })) => match key.sign(&nonce) {
                Ok(signature) => {
                    requests.write(SendRequest {
                        entity,
                        request: Request::Authenticate { signature },
                    });
                    commands.entity(entity).insert(Step::Prove);
                }
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(Rejected(crate::Failure::storage(error)));
                }
            },
            (Step::Prove, Ok(Response::Authenticated)) => {
                requests.write(SendRequest {
                    entity,
                    request: Request::ControlAccess {
                        environment: context.environment.clone(),
                    },
                });
                commands
                    .entity(entity)
                    .insert((Step::Lease, ProvenDevice(key.id)))
                    .remove::<DeviceKey>();
            }
            (_, Err(error)) => {
                commands
                    .entity(entity)
                    .insert(Rejected(crate::Failure::storage(error)));
            }
            _ => {
                commands
                    .entity(entity)
                    .insert(Rejected(crate::Failure::not_authenticated()));
            }
        }
        commands.entity(entity).remove::<Reply>();
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

type LeaseReplies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Context,
        &'static Step,
        &'static ProvenDevice,
        &'static mut Reply,
    ),
    With<Peer>,
>;

fn receive(
    mut peers: LeaseReplies,
    mut sources: Query<&mut Revision, With<ControlBroker>>,
    mut polls: MessageWriter<PollRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, owner, context, step, device, mut reply) in &mut peers {
        if !matches!(step, Step::Lease) {
            continue;
        }
        let Some(response) = reply.0.take() else {
            continue;
        };
        let (token, account_id, device_id, workspace_id, environment, expires_at) = match response {
            Ok(Response::ControlAccess {
                token,
                account_id,
                device_id,
                workspace_id,
                environment,
                expires_at,
            }) => (
                token,
                account_id,
                device_id,
                workspace_id,
                environment,
                expires_at,
            ),
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(crate::Failure::storage(error)));
                continue;
            }
            _ => {
                commands
                    .entity(entity)
                    .insert(Rejected(crate::Failure::access_denied()));
                continue;
            }
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(u64::MAX, |elapsed| elapsed.as_secs());
        if account_id != context.remote
            || device_id != device.0
            || environment != context.environment
            || expires_at <= now
            || expires_at > now.saturating_add(300)
        {
            commands
                .entity(entity)
                .insert(Rejected(crate::Failure::not_authenticated()));
            continue;
        }
        let Ok(credential) = Credential::from_lease(vmux_api::control::ControlLease {
            token,
            account_id,
            device_id,
            workspace_id,
            environment,
            expires_at,
        }) else {
            commands
                .entity(entity)
                .insert(Rejected(crate::Failure::not_authenticated()));
            continue;
        };
        let Ok(mut revision) = sources.get_mut(owner.parent()) else {
            continue;
        };
        let Some(next) = revision.0.checked_add(1) else {
            commands
                .entity(entity)
                .insert(Rejected(crate::Failure::not_authenticated()));
            continue;
        };
        revision.0 = next;
        commands
            .entity(owner.parent())
            .insert(credential)
            .remove::<Unavailable>();
        commands
            .entity(entity)
            .insert((
                Step::Wait,
                NextAt(Instant::now() + Duration::from_secs(120)),
            ))
            .remove::<Reply>();
        polls.write(PollRequest {
            entity,
            seconds: 60,
        });
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

type ReadyPeers<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Context, &'static Step, &'static NextAt),
    (With<Peer>, With<PollReady>),
>;

fn refresh(
    peers: ReadyPeers,
    mut requests: MessageWriter<SendRequest>,
    mut polls: MessageWriter<PollRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, context, step, next) in &peers {
        if matches!(step, Step::Retry) {
            if Instant::now() >= next.0 {
                commands.entity(entity).despawn();
            }
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
            continue;
        }
        if !matches!(step, Step::Wait) {
            continue;
        }
        commands.entity(entity).remove::<PollReady>();
        if Instant::now() < next.0 {
            polls.write(PollRequest {
                entity,
                seconds: 60,
            });
        } else {
            requests.write(SendRequest {
                entity,
                request: Request::ControlAccess {
                    environment: context.environment.clone(),
                },
            });
            commands.entity(entity).insert(Step::Lease);
        }
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn fail(
    peers: Query<(Entity, &ChildOf, &Rejected), With<Peer>>,
    mut polls: MessageWriter<PollRequest>,
    mut closes: MessageWriter<crate::cloud::transport::CloseRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, owner, rejected) in &peers {
        commands
            .entity(owner.parent())
            .remove::<Credential>()
            .insert(Unavailable(rejected.0.clone()));
        commands
            .entity(entity)
            .remove::<Rejected>()
            .remove::<Reply>()
            .insert((
                Step::Retry,
                NextAt(Instant::now() + Duration::from_secs(30)),
            ));
        closes.write(crate::cloud::transport::CloseRequest { entity });
        polls.write(PollRequest {
            entity,
            seconds: 30,
        });
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}
