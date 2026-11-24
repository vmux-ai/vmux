use std::time::{Duration, Instant};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use uuid::Uuid;
use vmux_ecs::{JobWake, Operation};

use crate::availability::SecureCredentials;
use crate::catalog::Catalog;
use crate::cloud::transport::{LocalCertificate, NativePeer, Reply};
use crate::connection::{
    DeviceKey, OpenRequest, PollReady, PollRequest, Rejected, SendRequest, TransportReady,
};
use crate::control::{StoredKeyPrepared, StoredKeyRequest};
use crate::wire::{Request, Response};
use crate::{Account, Binding, Location, Root, Server};

pub struct RelayAssertionPlugin;

impl bevy_app::Plugin for RelayAssertionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                retire, configure, stored, open, handshake, receive, refresh, fail,
            )
                .chain()
                .after(crate::Set::Publish),
        );
    }
}

#[derive(Component, Clone, PartialEq, Eq)]
pub struct RelayAssertion {
    pub account_id: Uuid,
    pub device_id: Uuid,
    pub compact: String,
    pub refresh_at: Instant,
}

impl RelayAssertion {
    const MIN_REFRESH_SECONDS: u64 = 60;
    const MAX_REFRESH_SECONDS: u64 = 3600;
    const POLL_SLICE_SECONDS: u64 = 60;
    const RETRY_SECONDS: u64 = 30;
}

#[derive(Component, Clone, PartialEq, Eq)]
pub struct RelayAssertionUnavailable(pub crate::Failure);

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
    #[allow(unknown_lints, vmux_ecs_ownership)]
    server: Server,
    data: std::path::PathBuf,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    certificate: Option<LocalCertificate>,
}

impl Context {
    fn matches(&self, other: &Self) -> bool {
        self.account == other.account
            && self.local == other.local
            && self.remote == other.remote
            && self.generation == other.generation
            && self.server == other.server
            && self.data == other.data
            && self.certificate == other.certificate
    }
}

#[derive(Component)]
enum Step {
    Key,
    Open,
    Challenge,
    Prove,
    Assert,
    Wait,
    Retry,
}

#[derive(Component)]
struct ProvenDevice(Uuid);

#[derive(Component)]
struct NextAt(Instant);

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
    fn context(&self, root: Entity) -> Option<Context> {
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
            certificate: certificate.cloned(),
        })
    }
}

fn retire(
    roots: Query<(Entity, Has<RelayAssertion>), With<Root>>,
    peers: Query<(Entity, &ChildOf, &Context), With<Peer>>,
    current: Current,
    mut commands: Commands,
) {
    for (root, has_assertion) in &roots {
        if has_assertion && current.context(root).is_none() {
            commands
                .entity(root)
                .remove::<RelayAssertion>()
                .remove::<RelayAssertionUnavailable>();
        }
    }
    for (entity, owner, context) in &peers {
        let valid = current
            .context(owner.parent())
            .is_some_and(|now| context.matches(&now));
        if valid {
            continue;
        }
        commands.entity(entity).despawn();
        if let Ok(mut root) = commands.get_entity(owner.parent()) {
            root.remove::<RelayAssertion>()
                .remove::<RelayAssertionUnavailable>();
        }
    }
}

fn configure(
    roots: Query<Entity, (With<Root>, With<SecureCredentials>)>,
    peers: Query<&ChildOf, With<Peer>>,
    current: Current,
    mut requests: MessageWriter<StoredKeyRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for root in &roots {
        if peers.iter().any(|owner| owner.parent() == root) {
            continue;
        }
        let Some(context) = current.context(root) else {
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
                Operation::bundle("account.relay_assertion").expect("static operation kind"),
                Peer,
                NativePeer,
                Step::Key,
                context,
                ChildOf(root),
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
    mut peers: Query<(Entity, &Step, &DeviceKey, &mut Reply), With<Peer>>,
    mut requests: MessageWriter<SendRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, step, key, mut reply) in &mut peers {
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
                    request: Request::Assertion,
                });
                commands
                    .entity(entity)
                    .insert((Step::Assert, ProvenDevice(key.id)))
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

type AssertionReplies<'w, 's> = Query<
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
    mut peers: AssertionReplies,
    mut polls: MessageWriter<PollRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, owner, context, step, device, mut reply) in &mut peers {
        if !matches!(step, Step::Assert) {
            continue;
        }
        let Some(response) = reply.0.take() else {
            continue;
        };
        let (assertion, refresh_in_seconds) = match response {
            Ok(Response::Asserted {
                assertion,
                refresh_in_seconds,
            }) => (assertion, refresh_in_seconds),
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(crate::Failure::storage(error)));
                continue;
            }
            _ => {
                commands
                    .entity(entity)
                    .insert(Rejected(crate::Failure::not_authenticated()));
                continue;
            }
        };
        let Some(compact) = assertion.compact() else {
            commands
                .entity(entity)
                .insert(Rejected(crate::Failure::not_authenticated()));
            continue;
        };
        let refresh_seconds = refresh_in_seconds.clamp(
            RelayAssertion::MIN_REFRESH_SECONDS,
            RelayAssertion::MAX_REFRESH_SECONDS,
        );
        let refresh_at = Instant::now() + Duration::from_secs(refresh_seconds);
        commands
            .entity(owner.parent())
            .insert(RelayAssertion {
                account_id: context.remote,
                device_id: device.0,
                compact,
                refresh_at,
            })
            .remove::<RelayAssertionUnavailable>();
        commands
            .entity(entity)
            .insert((Step::Wait, NextAt(refresh_at)))
            .remove::<Reply>();
        polls.write(PollRequest {
            entity,
            seconds: RelayAssertion::POLL_SLICE_SECONDS,
        });
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

type ReadyPeers<'w, 's> =
    Query<'w, 's, (Entity, &'static Step, &'static NextAt), (With<Peer>, With<PollReady>)>;

fn refresh(
    peers: ReadyPeers,
    mut requests: MessageWriter<SendRequest>,
    mut polls: MessageWriter<PollRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, step, next) in &peers {
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
                seconds: RelayAssertion::POLL_SLICE_SECONDS,
            });
        } else {
            requests.write(SendRequest {
                entity,
                request: Request::Assertion,
            });
            commands.entity(entity).insert(Step::Assert);
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
            .remove::<RelayAssertion>()
            .insert(RelayAssertionUnavailable(rejected.0.clone()));
        commands
            .entity(entity)
            .remove::<Rejected>()
            .remove::<Reply>()
            .insert((
                Step::Retry,
                NextAt(Instant::now() + Duration::from_secs(RelayAssertion::RETRY_SECONDS)),
            ));
        closes.write(crate::cloud::transport::CloseRequest { entity });
        polls.write(PollRequest {
            entity,
            seconds: RelayAssertion::RETRY_SECONDS,
        });
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}
