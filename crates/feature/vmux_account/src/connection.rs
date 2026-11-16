use std::fmt::Write;
use std::time::Instant;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use ring::signature::KeyPair;
use uuid::Uuid;
use vmux_ecs::record::Scope;
use vmux_ecs::{EntityTarget, JobSet, Operation, Ready, Revision};
use zeroize::Zeroizing;

use crate::catalog::Catalog;
use crate::wire::Request;
use crate::{
    Account, Binding, Failure, FailureKind, LinkRequest, Location, MutationCompleted, Root, Set,
};

pub(crate) struct ConnectionPlugin;

impl bevy_app::Plugin for ConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ConnectRequest>()
            .add_message::<CancelConnectRequest>()
            .add_message::<DisconnectRequest>()
            .add_message::<OpenRequest>()
            .add_message::<KeyRequest>()
            .add_message::<SendRequest>()
            .add_message::<PollRequest>()
            .configure_sets(
                Update,
                (Flow::Intent, Flow::Progress)
                    .chain()
                    .after(JobSet::Apply)
                    .before(Set::Receive),
            )
            .add_systems(
                Update,
                (cancel, disconnect, connect).chain().in_set(Flow::Intent),
            )
            .add_systems(
                Update,
                (
                    guard, key, prepare, authorize, poll, register, challenge, verify, lookup,
                    bind, reject,
                )
                    .chain()
                    .in_set(Flow::Progress),
            )
            .add_systems(Update, complete.after(Set::Reply).before(Set::Publish));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Flow {
    Intent,
    Progress,
}

#[derive(Component, Clone)]
#[component(immutable)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

impl Server {
    pub fn authority(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

#[derive(Message)]
pub struct ConnectRequest {
    pub root: Entity,
    pub account_id: Uuid,
}

#[derive(Message)]
pub struct DisconnectRequest {
    pub root: Entity,
    pub account_id: Uuid,
}

#[derive(Message)]
pub struct CancelConnectRequest {
    pub root: Entity,
    pub account_id: Uuid,
    pub connection_id: Uuid,
}

#[derive(Component)]
#[require(Phase)]
pub struct Connection;

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Connecting,
    Authorizing,
    AwaitingApproval,
    Registering,
    Challenging,
    Verifying,
    LookingUp,
    Saving,
    Connected,
}

#[derive(Component)]
pub struct Authorization {
    pub user_code: String,
    pub verification_uri: String,
}

#[derive(Component)]
pub struct ConnectionError(pub Failure);

impl ConnectionError {
    pub fn kind(&self) -> FailureKind {
        self.0.kind()
    }
}

#[derive(Component)]
pub struct Disconnecting;

#[derive(Component)]
pub struct CloudAvailable;

#[derive(Message)]
#[cfg_attr(not(feature = "cloud"), allow(dead_code))]
pub(crate) struct OpenRequest {
    pub entity: Entity,
    pub server: Server,
}

#[derive(Message)]
#[cfg_attr(not(feature = "cloud"), allow(dead_code))]
pub(crate) struct KeyRequest {
    pub entity: Entity,
    #[cfg_attr(all(feature = "cloud", not(target_os = "macos")), expect(dead_code))]
    pub data: std::path::PathBuf,
    #[cfg_attr(all(feature = "cloud", not(target_os = "macos")), expect(dead_code))]
    pub scope: String,
}

#[derive(Message)]
#[cfg_attr(not(feature = "cloud"), allow(dead_code))]
pub(crate) struct SendRequest {
    pub entity: Entity,
    pub request: Request,
}

#[derive(Message)]
#[cfg_attr(not(feature = "cloud"), allow(dead_code))]
pub(crate) struct PollRequest {
    pub entity: Entity,
    pub seconds: u64,
}

#[derive(Component)]
pub(crate) struct DeviceKey {
    pub id: Uuid,
    pub seed: Zeroizing<[u8; 32]>,
    pub reused: bool,
}

impl DeviceKey {
    fn public_key(&self) -> Result<String, String> {
        let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(self.seed.as_ref())
            .map_err(|_| "invalid device key")?;
        Ok(Hex::encode(key.public_key().as_ref()))
    }

    fn sign(&self, nonce: &str) -> Result<String, String> {
        let nonce = Hex::decode(nonce)?;
        if nonce.len() != 32 {
            return Err("invalid account challenge".into());
        }
        let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(self.seed.as_ref())
            .map_err(|_| "invalid device key")?;
        Ok(Hex::encode(key.sign(&nonce).as_ref()))
    }
}

pub(crate) struct Hex;

impl Hex {
    pub(crate) fn encode(bytes: &[u8]) -> String {
        let mut text = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            let _ = write!(text, "{byte:02x}");
        }
        text
    }

    fn decode(text: &str) -> Result<Vec<u8>, String> {
        if text.len() != 64 || !text.is_ascii() {
            return Err("invalid account challenge".into());
        }
        let mut bytes = Vec::with_capacity(32);
        for offset in (0..text.len()).step_by(2) {
            bytes.push(
                u8::from_str_radix(&text[offset..offset + 2], 16)
                    .map_err(|_| "invalid account challenge")?,
            );
        }
        Ok(bytes)
    }
}

#[derive(Component)]
pub(crate) struct KeyPrepared(pub Option<Result<DeviceKey, String>>);

#[derive(Component)]
pub(crate) struct TransportReady;

#[derive(Component)]
pub(crate) struct Grant {
    pub code: String,
    pub expires: Instant,
    pub interval: u64,
}

#[derive(Component)]
pub(crate) struct Authorized;

#[derive(Component)]
pub(crate) struct Waiting;

#[derive(Component)]
pub(crate) struct Slowed;

#[derive(Component)]
pub(crate) struct PollReady;

#[derive(Component)]
pub(crate) struct SessionToken(pub String);

#[derive(Component)]
pub(crate) struct Registered;

#[derive(Component)]
pub(crate) struct Challenge(pub String);

#[derive(Component)]
pub(crate) struct Verified;

#[derive(Component)]
pub(crate) struct RemoteAccount(pub Uuid);

#[derive(Component)]
pub(crate) struct Rejected(pub Failure);

#[derive(Component)]
struct Reusing;

#[derive(Component)]
struct PreparingKey;

#[derive(Component)]
struct PendingBinding(Uuid);

#[derive(Component)]
struct PendingDisconnect(Uuid);

fn cancel(
    mut requests: MessageReader<CancelConnectRequest>,
    catalog: Catalog,
    connections: Cancelable,
    mut commands: Commands,
) {
    for request in requests.read() {
        if catalog
            .account(request.root)
            .is_none_or(|(_, id)| id != request.account_id)
        {
            continue;
        }
        for (entity, owner, id) in &connections {
            if owner.parent() == request.root && id.0 == request.connection_id.to_string() {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn connect(
    mut requests: MessageReader<ConnectRequest>,
    catalog: Catalog,
    roots: Configured,
    account_state: Query<(&Revision, Option<&Binding>), With<Account>>,
    connections: Query<(Entity, &ChildOf), With<Connection>>,
    mut opens: MessageWriter<OpenRequest>,
    mut commands: Commands,
) {
    let mut started = std::collections::HashSet::new();
    for request in requests.read() {
        let Some((account, id)) = catalog.account(request.root) else {
            continue;
        };
        if id != request.account_id || !started.insert(request.root) {
            continue;
        }
        let Ok((server, location)) = roots.get(request.root) else {
            commands
                .entity(request.root)
                .insert(ConnectionError(Failure::access_denied()));
            continue;
        };
        for (entity, owner) in &connections {
            if owner.parent() == request.root {
                commands.entity(entity).despawn();
            }
        }
        let (revision, binding) = account_state.get(account).unwrap();
        let generation = match binding {
            Some(binding) if binding.is_linked() => binding.credential_generation,
            _ => revision.0,
        };
        let entity = commands
            .spawn((
                Operation::bundle("account.connect").unwrap(),
                Connection,
                Phase::Connecting,
                Scope(id),
                Revision(generation),
                EntityTarget::<Account>::new(account),
                ChildOf(request.root),
                server.clone(),
                Location(location.0.clone()),
            ))
            .id();
        commands.entity(request.root).remove::<ConnectionError>();
        opens.write(OpenRequest {
            entity,
            server: server.clone(),
        });
    }
}

fn disconnect(
    mut requests: MessageReader<DisconnectRequest>,
    catalog: Catalog,
    connections: Query<(Entity, &ChildOf), With<Connection>>,
    mut links: MessageWriter<LinkRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if catalog
            .account(request.root)
            .is_none_or(|(_, id)| id != request.account_id)
        {
            continue;
        }
        for (entity, owner) in &connections {
            if owner.parent() == request.root {
                commands.entity(entity).despawn();
            }
        }
        let operation_id = Uuid::new_v4();
        commands
            .entity(request.root)
            .insert((PendingDisconnect(operation_id), Disconnecting))
            .remove::<ConnectionError>();
        links.write(LinkRequest {
            root: request.root,
            operation_id,
            account_id: request.account_id,
            binding: Binding::default(),
        });
    }
}

fn guard(
    connections: Sessions,
    accounts: Query<(&vmux_ecs::Uuid, Option<&Binding>), With<Account>>,
    mut commands: Commands,
) {
    for (entity, target, scope, phase, server, generation, remote) in &connections {
        let Ok((id, binding)) = accounts.get(target.entity()) else {
            commands.entity(entity).despawn();
            continue;
        };
        if id.0 != scope.0.to_string() {
            commands.entity(entity).despawn();
            continue;
        }
        if *phase == Phase::Connected
            && !binding.is_some_and(|binding| {
                binding.server == server.authority()
                    && binding.credential_generation == generation.0
                    && remote.is_some_and(|remote| {
                        binding.account_id.as_deref() == Some(remote.0.to_string().as_str())
                    })
            })
        {
            commands.entity(entity).despawn();
        }
    }
}

fn key(connected: KeyInputs, mut requests: MessageWriter<KeyRequest>, mut commands: Commands) {
    for (entity, scope, server, generation, location) in &connected {
        requests.write(KeyRequest {
            entity,
            data: location.0.clone(),
            scope: format!("{}/{}/{}", scope.0, server.authority(), generation.0),
        });
        commands.entity(entity).insert(PreparingKey);
    }
}

fn prepare(mut ready: PreparedKeys, mut send: MessageWriter<SendRequest>, mut commands: Commands) {
    for (entity, mut prepared) in &mut ready {
        let Some(result) = prepared.0.take() else {
            continue;
        };
        let key = match result {
            Ok(key) => key,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(Failure::storage(error)));
                continue;
            }
        };
        let request = if key.reused {
            commands
                .entity(entity)
                .insert((Reusing, Phase::Challenging));
            Request::Challenge {
                device_id: key.id.to_string(),
            }
        } else {
            commands.entity(entity).insert(Phase::Authorizing);
            Request::AuthorizeDevice
        };
        commands.entity(entity).insert(key).remove::<KeyPrepared>();
        send.write(SendRequest { entity, request });
    }
}

fn authorize(
    ready: Query<(Entity, &Phase, &Grant, &Authorization), With<Authorized>>,
    mut polls: MessageWriter<PollRequest>,
    mut commands: Commands,
) {
    for (entity, phase, grant, authorization) in &ready {
        if *phase != Phase::Authorizing {
            continue;
        }
        let valid = url::Url::parse(&authorization.verification_uri).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        });
        if !valid
            || authorization.user_code.is_empty()
            || authorization.user_code.len() > 64
            || grant.code.is_empty()
            || grant.code.len() > 4096
            || grant.interval == 0
            || grant.interval > 60
        {
            commands
                .entity(entity)
                .insert(Rejected(Failure::invalid_store()));
            continue;
        }
        commands
            .entity(entity)
            .insert(Phase::AwaitingApproval)
            .remove::<Authorized>();
        polls.write(PollRequest {
            entity,
            seconds: grant.interval,
        });
    }
}

fn poll(
    mut pending: PendingPoll,
    mut polls: MessageWriter<PollRequest>,
    mut send: MessageWriter<SendRequest>,
    mut commands: Commands,
) {
    for (entity, phase, mut grant, waiting, slowed, ready) in &mut pending {
        if *phase != Phase::AwaitingApproval {
            continue;
        }
        if Instant::now() >= grant.expires {
            commands
                .entity(entity)
                .insert(Rejected(Failure::authorization_expired()));
            continue;
        }
        if waiting || slowed {
            if slowed {
                grant.interval = (grant.interval + 5).min(60);
            }
            commands
                .entity(entity)
                .remove::<Waiting>()
                .remove::<Slowed>();
            polls.write(PollRequest {
                entity,
                seconds: grant.interval,
            });
        } else if ready {
            commands.entity(entity).remove::<PollReady>();
            send.write(SendRequest {
                entity,
                request: Request::Token {
                    device_code: grant.code.clone(),
                },
            });
        }
    }
}

fn register(
    ready: Query<(Entity, &Phase, &SessionToken, &DeviceKey)>,
    mut send: MessageWriter<SendRequest>,
    mut commands: Commands,
) {
    for (entity, phase, token, key) in &ready {
        if *phase != Phase::AwaitingApproval {
            continue;
        }
        let public_key = match key.public_key() {
            Ok(key) => key,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(Failure::storage(error)));
                continue;
            }
        };
        send.write(SendRequest {
            entity,
            request: Request::RegisterDevice {
                session_token: token.0.clone(),
                device_id: key.id.to_string(),
                kind: "desktop".into(),
                name: vmux_profile::Product::display_name(),
                public_key,
            },
        });
        commands
            .entity(entity)
            .insert(Phase::Registering)
            .remove::<SessionToken>()
            .remove::<Authorization>()
            .remove::<Grant>();
    }
}

fn challenge(
    ready: Query<(Entity, &Phase, &DeviceKey), With<Registered>>,
    mut send: MessageWriter<SendRequest>,
    mut commands: Commands,
) {
    for (entity, phase, key) in &ready {
        if *phase != Phase::Registering {
            continue;
        }
        commands
            .entity(entity)
            .insert(Phase::Challenging)
            .remove::<Registered>();
        send.write(SendRequest {
            entity,
            request: Request::Challenge {
                device_id: key.id.to_string(),
            },
        });
    }
}

fn verify(
    ready: Query<(Entity, &Phase, &Challenge, &DeviceKey)>,
    mut send: MessageWriter<SendRequest>,
    mut commands: Commands,
) {
    for (entity, phase, challenge, key) in &ready {
        if *phase != Phase::Challenging {
            continue;
        }
        match key.sign(&challenge.0) {
            Ok(signature) => {
                commands
                    .entity(entity)
                    .insert(Phase::Verifying)
                    .remove::<Challenge>();
                send.write(SendRequest {
                    entity,
                    request: Request::Authenticate { signature },
                });
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(Failure::storage(error)));
            }
        }
    }
}

fn lookup(
    ready: Query<(Entity, &Phase), With<Verified>>,
    mut send: MessageWriter<SendRequest>,
    mut commands: Commands,
) {
    for (entity, phase) in &ready {
        if *phase != Phase::Verifying {
            continue;
        }
        commands
            .entity(entity)
            .insert(Phase::LookingUp)
            .remove::<Verified>()
            .remove::<Reusing>();
        send.write(SendRequest {
            entity,
            request: Request::Account,
        });
    }
}

fn bind(
    ready: Accounts,
    catalog: Catalog,
    mut links: MessageWriter<LinkRequest>,
    mut commands: Commands,
) {
    for (entity, owner, scope, server, generation, phase, account) in &ready {
        if *phase != Phase::LookingUp {
            continue;
        }
        if account.0.is_nil()
            || catalog
                .account(owner.parent())
                .is_none_or(|(_, id)| id != scope.0)
        {
            commands
                .entity(entity)
                .insert(Rejected(Failure::account_mismatch()));
            continue;
        }
        let operation_id = Uuid::new_v4();
        links.write(LinkRequest {
            root: owner.parent(),
            operation_id,
            account_id: scope.0,
            binding: Binding {
                server: server.authority(),
                account_id: Some(account.0.to_string()),
                credential_generation: generation.0,
            },
        });
        commands
            .entity(entity)
            .insert((Phase::Saving, PendingBinding(operation_id)));
    }
}

fn reject(rejected: Rejections, mut send: MessageWriter<SendRequest>, mut commands: Commands) {
    for (entity, owner, phase, rejected, reusing) in &rejected {
        if reusing
            && matches!(phase, Phase::Challenging | Phase::Verifying)
            && rejected.0 == Failure::not_authenticated()
        {
            commands
                .entity(entity)
                .insert(Phase::Authorizing)
                .remove::<Rejected>()
                .remove::<Reusing>();
            send.write(SendRequest {
                entity,
                request: Request::AuthorizeDevice,
            });
        } else {
            commands
                .entity(owner.parent())
                .insert(ConnectionError(rejected.0.clone()));
            commands.entity(entity).despawn();
        }
    }
}

fn complete(
    mut results: MessageReader<MutationCompleted>,
    connections: BindingReplies,
    disconnects: Query<(Entity, &PendingDisconnect), With<Root>>,
    catalog: Catalog,
    bindings: Query<&Binding, With<Account>>,
    mut commands: Commands,
) {
    for result in results.read() {
        for (entity, owner, pending, server, generation, remote) in &connections {
            if owner.parent() != result.root || pending.0 != result.operation_id {
                continue;
            }
            match &result.result {
                Ok(_) => {
                    let bound = catalog.account(result.root).is_some_and(|(account, _)| {
                        bindings.get(account).is_ok_and(|binding| {
                            binding.server == server.authority()
                                && binding.credential_generation == generation.0
                                && binding.account_id.as_deref()
                                    == Some(remote.0.to_string().as_str())
                        })
                    });
                    if !bound {
                        commands
                            .entity(result.root)
                            .insert(ConnectionError(Failure::account_mismatch()));
                        commands.entity(entity).despawn();
                        continue;
                    }
                    commands
                        .entity(entity)
                        .insert(Phase::Connected)
                        .remove::<PendingBinding>();
                }
                Err(error) => {
                    commands
                        .entity(result.root)
                        .insert(ConnectionError(error.clone()));
                    commands.entity(entity).despawn();
                }
            }
        }
        for (root, pending) in &disconnects {
            if root != result.root || pending.0 != result.operation_id {
                continue;
            }
            commands
                .entity(root)
                .remove::<PendingDisconnect>()
                .remove::<Disconnecting>();
            if let Err(error) = &result.result {
                commands.entity(root).insert(ConnectionError(error.clone()));
            }
        }
    }
}

type Cancelable<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static ChildOf, &'static vmux_ecs::Uuid),
    (With<Connection>, Without<PendingBinding>),
>;

type KeyInputs<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Scope,
        &'static Server,
        &'static Revision,
        &'static Location,
    ),
    (
        With<Connection>,
        With<TransportReady>,
        Without<PreparingKey>,
    ),
>;

type Configured<'w, 's> = Query<
    'w,
    's,
    (&'static Server, &'static Location),
    (
        With<Root>,
        With<Ready>,
        With<CloudAvailable>,
        Without<Disconnecting>,
    ),
>;

type Sessions<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static EntityTarget<Account>,
        &'static Scope,
        &'static Phase,
        &'static Server,
        &'static Revision,
        Option<&'static RemoteAccount>,
    ),
    With<Connection>,
>;

type PreparedKeys<'w, 's> =
    Query<'w, 's, (Entity, &'static mut KeyPrepared), (With<Connection>, With<TransportReady>)>;

type PendingPoll<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Phase,
        &'static mut Grant,
        Has<Waiting>,
        Has<Slowed>,
        Has<PollReady>,
    ),
>;

type Accounts<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Scope,
        &'static Server,
        &'static Revision,
        &'static Phase,
        &'static RemoteAccount,
    ),
>;

type Rejections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Phase,
        &'static Rejected,
        Has<Reusing>,
    ),
    With<Connection>,
>;

type BindingReplies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static PendingBinding,
        &'static Server,
        &'static Revision,
        &'static RemoteAccount,
    ),
    With<Connection>,
>;
