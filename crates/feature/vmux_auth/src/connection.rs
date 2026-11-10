use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use vmux_api::Id;
use vmux_connector::{AccountExternalId, CanAccess, ProviderKey};
use vmux_ecs::{
    BlockingRuntime, BlockingTask, Operation, OperationError, OperationKind, OperationState,
};
use vmux_profile::OauthCredentialStorage;
use vmux_profile::oauth::OauthCredentialAccess;

use crate::flow::{AuthFlow, AuthProgress, Cancellation, Revocation};
use crate::provider::AuthProviderDefinition;

const AUTH_WORKER_COUNT: usize = 2;
const AUTH_QUEUE_CAPACITY: usize = 8;
const MAX_CONNECTION_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_CONNECTIONS: usize = 128;

type AuthWork = Box<dyn FnOnce() + Send + 'static>;

pub struct ConnectionPlugin;

impl Plugin for ConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockingRuntime>()
            .init_resource::<AuthRuntime>()
            .register_type::<Connection>()
            .register_type::<Id<Connection>>()
            .register_type::<ProviderKey>()
            .register_type::<ConnectionName>()
            .register_type::<AccountExternalId>()
            .register_type::<CanAccess>()
            .register_type::<ConnectionState>()
            .init_resource::<ConnectionPersistence>()
            .add_message::<ConnectRequest>()
            .add_message::<DisconnectRequest>()
            .add_systems(PostStartup, (initialize_store, start_restore).chain())
            .add_systems(
                Update,
                (retry_disconnects, disconnect)
                    .chain()
                    .before(update_operations),
            )
            .add_systems(
                Update,
                (
                    start_restore,
                    finish_restore,
                    finish_validation,
                    clear_failed_validation,
                    connect,
                    update_operations,
                    finish_disconnect,
                ),
            )
            .add_systems(Update, (finish_persist, persist).chain());
    }
}

#[derive(Resource)]
struct AuthRuntime {
    sender: Option<mpsc::SyncSender<AuthWork>>,
}

impl Default for AuthRuntime {
    fn default() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<AuthWork>(AUTH_QUEUE_CAPACITY);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = 0;
        for index in 0..AUTH_WORKER_COUNT {
            let receiver = Arc::clone(&receiver);
            let spawned = std::thread::Builder::new()
                .name(format!("vmux-auth-{index}"))
                .spawn(move || {
                    loop {
                        let work = receiver.lock().recv();
                        let Ok(work) = work else {
                            return;
                        };
                        let _ = catch_unwind(AssertUnwindSafe(work));
                    }
                });
            if spawned.is_ok() {
                workers += 1;
            }
        }
        Self {
            sender: (workers > 0).then_some(sender),
        }
    }
}

impl AuthRuntime {
    fn spawn(&self, work: impl FnOnce() + Send + 'static) -> Result<(), String> {
        let Some(sender) = &self.sender else {
            return Err("OAuth executor is unavailable".to_string());
        };
        sender
            .try_send(Box::new(work))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "OAuth operation queue is full".to_string(),
                mpsc::TrySendError::Disconnected(_) => "OAuth executor is unavailable".to_string(),
            })
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_auth"]
pub struct Connection;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_auth"]
pub struct ConnectionName(pub String);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_auth"]
pub enum ConnectionState {
    #[default]
    Connecting,
    AwaitingUser {
        verification_uri: String,
        user_code: String,
        expires_at: i64,
    },
    Connected,
    Failed(String),
    Disconnected,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct Connected {
    #[event_target]
    pub entity: Entity,
    pub id: Id<Connection>,
    pub provider: ProviderKey,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct Disconnected {
    #[event_target]
    pub entity: Entity,
    pub id: Id<Connection>,
    pub provider: ProviderKey,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct ConnectRequest {
    pub provider: ProviderKey,
}

impl ConnectRequest {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: ProviderKey(provider.into()),
        }
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct DisconnectRequest {
    pub connection: Id<Connection>,
}

#[derive(Component)]
struct ConnectTask {
    connection: Entity,
    id: Id<Connection>,
    provider: ProviderKey,
    progress: Mutex<mpsc::Receiver<AuthProgress>>,
    result: Mutex<mpsc::Receiver<Result<ConnectOutcome, String>>>,
    cancellation: Cancellation,
}

struct ConnectTaskInput {
    connection: Entity,
    id: Id<Connection>,
    provider: ProviderKey,
    definition: vmux_ecs::manifest::AuthProvider,
    credential_revision: u64,
    credentials: CredentialStore,
    wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
}

enum ConnectOutcome {
    Connected,
    Canceled,
    DisconnectPending(String),
}

#[derive(Component)]
struct DisconnectTask {
    connection: Entity,
    id: Id<Connection>,
    provider: ProviderKey,
    result: Mutex<mpsc::Receiver<Result<Revocation, String>>>,
}

#[derive(Component)]
struct DisconnectRequested;

#[derive(Component)]
struct DisconnectRetryAt(Instant);

#[derive(Component)]
pub(crate) struct ConnectionValidationPending {
    name: String,
    account_external_id: String,
    scopes: Vec<String>,
}

#[derive(Component)]
struct ConnectionLoadTask(BlockingTask<Result<Vec<RestoredConnection>, String>>);

#[derive(Component)]
struct ConnectionPersistTask(BlockingTask<Result<(), String>>);

impl ConnectTask {
    fn spawn(runtime: &AuthRuntime, input: ConnectTaskInput) -> Result<Self, String> {
        let ConnectTaskInput {
            connection,
            id,
            provider,
            definition,
            credential_revision,
            credentials,
            wake,
        } = input;
        let (progress_sender, progress_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let cancellation = Cancellation::default();
        let worker_cancellation = cancellation.clone();
        let worker_id = id.clone();
        let worker_provider = provider.clone();
        runtime.spawn(move || {
            let result = AuthFlow::connect(&definition, &worker_cancellation, |progress| {
                let _ = progress_sender.send(progress);
                if let Some(wake) = &wake {
                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                }
            })
            .and_then(|credentials_value| {
                if worker_cancellation.is_canceled() {
                    return credentials.discard_issued(
                        &worker_provider,
                        &worker_id,
                        &definition,
                        &credentials_value,
                        None,
                    );
                }
                let access = credentials.access(&worker_provider, &worker_id);
                let stored = access.write_if_revision(credential_revision, || {
                    credentials.storage.store(
                        &worker_provider.0,
                        worker_id.as_str(),
                        &credentials_value,
                    )
                });
                match stored {
                    Ok(Some(())) if !worker_cancellation.is_canceled() => {
                        Ok(ConnectOutcome::Connected)
                    }
                    Ok(_) => credentials.discard_issued(
                        &worker_provider,
                        &worker_id,
                        &definition,
                        &credentials_value,
                        None,
                    ),
                    Err(error) => credentials.discard_issued(
                        &worker_provider,
                        &worker_id,
                        &definition,
                        &credentials_value,
                        Some(error),
                    ),
                }
            });
            let _ = result_sender.send(result);
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        })?;
        Ok(Self {
            connection,
            id,
            provider,
            progress: Mutex::new(progress_receiver),
            result: Mutex::new(result_receiver),
            cancellation,
        })
    }

    fn cancel(&self) {
        self.cancellation.cancel();
    }
}

impl Drop for ConnectTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

type CredentialAccessMap =
    Arc<Mutex<HashMap<(ProviderKey, Id<Connection>), OauthCredentialAccess>>>;

#[derive(Component, Clone, Debug)]
pub(crate) struct CredentialStore {
    access: CredentialAccessMap,
    pub(crate) storage: OauthCredentialStorage,
}

impl CredentialStore {
    pub(crate) fn access(
        &self,
        provider: &ProviderKey,
        connection: &Id<Connection>,
    ) -> OauthCredentialAccess {
        let key = (provider.clone(), connection.clone());
        self.access.lock().entry(key).or_default().clone()
    }

    fn remove_access(&self, provider: &ProviderKey, connection: &Id<Connection>) {
        self.access
            .lock()
            .remove(&(provider.clone(), connection.clone()));
    }

    fn discard_issued(
        &self,
        provider: &ProviderKey,
        connection: &Id<Connection>,
        definition: &vmux_ecs::manifest::AuthProvider,
        credentials: &vmux_profile::oauth::OauthCredentials,
        failure: Option<String>,
    ) -> Result<ConnectOutcome, String> {
        let access = self.access(provider, connection);
        access.invalidate();
        let mut errors = failure.into_iter().collect::<Vec<_>>();
        let durable = access.cleanup(|| {
            self.storage
                .store(&provider.0, connection.as_str(), credentials)
        });
        if let Err(error) = &durable {
            errors.push(format!("OAuth credentials could not be retained: {error}"));
        }
        if let Err(error) = AuthFlow::revoke_retrying(definition, credentials, 3) {
            errors.push(format!("OAuth credentials could not be revoked: {error}"));
            if durable.is_ok() {
                return Ok(ConnectOutcome::DisconnectPending(errors.join("; ")));
            }
            return Err(errors.join("; "));
        }
        if let Err(error) = access.cleanup(|| self.storage.remove(&provider.0, connection.as_str()))
        {
            errors.push(format!("OAuth credentials could not be removed: {error}"));
            if durable.is_ok() {
                return Ok(ConnectOutcome::DisconnectPending(errors.join("; ")));
            }
        }
        if errors.is_empty() {
            Ok(ConnectOutcome::Canceled)
        } else {
            Err(errors.join("; "))
        }
    }
}

#[derive(Resource, Default)]
struct ConnectionPersistence {
    dirty: bool,
    blocked: bool,
    restore_running: bool,
    restore_complete: bool,
    restore_retry_at: Option<Instant>,
    retry_at: Option<Instant>,
    last_error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ConnectionFile {
    version: u32,
    connections: Vec<ConnectionRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ConnectionRecord {
    id: String,
    provider: String,
    name: String,
    account_external_id: String,
    scopes: Vec<String>,
    #[serde(default)]
    disconnected: bool,
}

struct RestoredConnection {
    record: ConnectionRecord,
    state: ConnectionState,
}

impl ConnectionFile {
    fn restore(
        path: &std::path::Path,
        storage: &OauthCredentialStorage,
    ) -> Result<Vec<RestoredConnection>, String> {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let mut source = String::new();
        file.take(MAX_CONNECTION_METADATA_BYTES + 1)
            .read_to_string(&mut source)
            .map_err(|error| error.to_string())?;
        if source.len() as u64 > MAX_CONNECTION_METADATA_BYTES {
            return Err("Connection metadata is too large and was preserved".to_string());
        }
        let file = ron::from_str::<Self>(&source)
            .map_err(|error| format!("Invalid connection metadata was preserved: {error}"))?;
        if file.version != 1 {
            return Err(format!(
                "Unsupported connection metadata version {} was preserved",
                file.version
            ));
        }
        if file.connections.len() > MAX_CONNECTIONS {
            return Err("Connection metadata has too many records and was preserved".to_string());
        }
        let mut restored = Vec::with_capacity(file.connections.len());
        for record in file.connections {
            let state = if record.disconnected {
                ConnectionState::Disconnected
            } else {
                match storage.load_retrying(&record.provider, &record.id, 3) {
                    Ok(Some(_)) => ConnectionState::Connected,
                    Ok(None) => ConnectionState::Disconnected,
                    Err(error) => ConnectionState::Failed(error),
                }
            };
            restored.push(RestoredConnection { record, state });
        }
        Ok(restored)
    }
}

fn initialize_store(
    profiles: Query<
        (
            Entity,
            &vmux_ecs::profile::ActiveProfile,
            &vmux_ecs::profile::ProfileDirectories,
        ),
        Without<CredentialStore>,
    >,
    mut commands: Commands,
) {
    for (entity, profile, directories) in &profiles {
        commands.entity(entity).insert(CredentialStore {
            access: Arc::new(Mutex::new(HashMap::new())),
            storage: OauthCredentialStorage::at(profile.0.clone(), directories.0.clone()),
        });
    }
}

fn start_restore(
    profiles: Query<(&vmux_ecs::profile::ProfileDirectories, &CredentialStore)>,
    runtime: Res<BlockingRuntime>,
    wake: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut persistence: ResMut<ConnectionPersistence>,
    mut commands: Commands,
) {
    if persistence.restore_complete
        || persistence.restore_running
        || persistence
            .restore_retry_at
            .is_some_and(|retry_at| retry_at > Instant::now())
    {
        return;
    }
    let Ok((profile, credentials)) = profiles.single() else {
        return;
    };
    let path = profile.0.profile().join("connections.ron");
    let storage = credentials.storage.clone();
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    let Some(task) = runtime.spawn(wake, move || ConnectionFile::restore(&path, &storage)) else {
        persistence.restore_retry_at = Some(Instant::now() + Duration::from_secs(1));
        commands.spawn((
            Operation,
            vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
            vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
            OperationKind::new("auth.connection.restore").expect("valid operation metadata"),
            OperationState::Failed,
            OperationError::new("Connection persistence queue is full")
                .expect("valid operation metadata"),
        ));
        return;
    };
    persistence.restore_running = true;
    persistence.restore_retry_at = None;
    commands.spawn((
        Operation,
        vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
        OperationKind::new("auth.connection.restore").expect("valid operation metadata"),
        OperationState::Running,
        ConnectionLoadTask(task),
    ));
}

fn finish_restore(
    mut tasks: Query<(Entity, &mut ConnectionLoadTask)>,
    providers: Query<(&ProviderKey, &AuthProviderDefinition)>,
    stores: Query<&CredentialStore>,
    mut disconnects: MessageWriter<DisconnectRequest>,
    mut persistence: ResMut<ConnectionPersistence>,
    mut commands: Commands,
) {
    let store = stores.single().ok();
    for (operation, mut task) in &mut tasks {
        let Some(result) = task.0.poll() else {
            continue;
        };
        persistence.restore_running = false;
        commands.entity(operation).remove::<ConnectionLoadTask>();
        let restored = match result {
            Ok(Ok(restored)) => restored,
            Ok(Err(error)) => {
                persistence.blocked = true;
                persistence.restore_complete = true;
                commands.entity(operation).insert((
                    OperationState::Failed,
                    OperationError::new(error).expect("valid operation metadata"),
                ));
                continue;
            }
            Err(_) => {
                persistence.restore_retry_at = Some(Instant::now() + Duration::from_secs(1));
                commands.entity(operation).insert((
                    OperationState::Failed,
                    OperationError::new("Connection restore panicked")
                        .expect("valid operation metadata"),
                ));
                continue;
            }
        };
        for restored in restored {
            let record = restored.record;
            let disconnected = record.disconnected;
            let definition = providers
                .iter()
                .find_map(|(key, definition)| (key.0 == record.provider).then_some(definition));
            let state = if definition.is_some() {
                restored.state
            } else {
                ConnectionState::Failed(format!("Unknown auth provider: {}", record.provider))
            };
            let id = Id::<Connection>::new(record.id);
            let provider = ProviderKey(record.provider);
            let connected = state == ConnectionState::Connected;
            let provider_name = definition
                .map(|definition| definition.0.name.clone())
                .unwrap_or_else(|| record.name.clone());
            let mut connection = commands.spawn((Connection, id.clone(), provider.clone(), state));
            if connected {
                connection.insert((
                    ConnectionName(provider_name),
                    AccountExternalId::default(),
                    CanAccess::default(),
                    ConnectionValidationPending {
                        name: record.name,
                        account_external_id: record.account_external_id,
                        scopes: record.scopes,
                    },
                ));
                if let Some(store) = store {
                    let access = store.access(&provider, &id);
                    if !access.is_active() {
                        continue;
                    }
                }
                let entity = connection.id();
                commands.trigger(Connected {
                    entity,
                    id,
                    provider,
                });
            } else {
                connection.insert((
                    ConnectionName(record.name),
                    AccountExternalId(record.account_external_id),
                    CanAccess(record.scopes.into_iter().map(Id::new).collect()),
                ));
                if disconnected {
                    connection.insert(DisconnectRequested);
                    disconnects.write(DisconnectRequest {
                        connection: id.clone(),
                    });
                }
            }
        }
        persistence.blocked = false;
        persistence.restore_complete = true;
        commands.entity(operation).insert(OperationState::Succeeded);
    }
}

type ValidatingConnections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id<Connection>,
        &'static ProviderKey,
        &'static ConnectionValidationPending,
        &'static AccountExternalId,
        &'static mut CanAccess,
        &'static mut ConnectionState,
    ),
    (With<Connection>, With<ConnectionValidationPending>),
>;

fn finish_validation(
    mut connections: ValidatingConnections,
    credentials: Query<&CredentialStore>,
    mut commands: Commands,
) {
    let store = credentials.single().ok();
    for (entity, id, provider, pending, account, mut access, mut state) in &mut connections {
        if account.0.is_empty() && access.0.is_empty() {
            continue;
        }
        if !pending.account_external_id.is_empty() && pending.account_external_id != account.0 {
            if let Some(store) = store {
                store.access(provider, id).invalidate();
            }
            *state = ConnectionState::Failed("OAuth account identity changed".to_string());
            commands.entity(entity).insert((
                AccountExternalId(pending.account_external_id.clone()),
                CanAccess::default(),
            ));
            commands.spawn((
                Operation,
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                OperationKind::new("auth.connection.validate").expect("valid operation metadata"),
                OperationState::Failed,
                OperationError::new("OAuth account identity changed")
                    .expect("valid operation metadata"),
            ));
        } else {
            for scope in &pending.scopes {
                let scope = Id::new(scope);
                if !access.0.contains(&scope) {
                    access.0.push(scope);
                }
            }
        }
        commands
            .entity(entity)
            .remove::<ConnectionValidationPending>();
    }
}

type FailedValidations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id<Connection>,
        &'static ProviderKey,
        &'static ConnectionState,
    ),
    (With<ConnectionValidationPending>, Changed<ConnectionState>),
>;

fn clear_failed_validation(
    connections: FailedValidations,
    credentials: Query<&CredentialStore>,
    mut commands: Commands,
) {
    let store = credentials.single().ok();
    for (entity, id, provider, state) in &connections {
        if !matches!(state, ConnectionState::Failed(_)) {
            continue;
        }
        if let Some(store) = store {
            store.access(provider, id).invalidate();
        }
        commands
            .entity(entity)
            .remove::<ConnectionValidationPending>();
    }
}

type PersistedConnections<'w, 's> = Query<
    'w,
    's,
    (
        &'static Id<Connection>,
        &'static ProviderKey,
        &'static ConnectionName,
        Option<&'static AccountExternalId>,
        Option<&'static CanAccess>,
        Option<&'static ConnectionValidationPending>,
        &'static ConnectionState,
        Option<&'static DisconnectRequested>,
    ),
    With<Connection>,
>;

type ChangedPersistedConnections<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<Connection>,
        Or<(
            Changed<Id<Connection>>,
            Changed<ProviderKey>,
            Changed<ConnectionName>,
            Changed<AccountExternalId>,
            Changed<CanAccess>,
            Changed<ConnectionState>,
            Changed<DisconnectRequested>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct PersistContext<'w, 's> {
    profiles: Query<'w, 's, &'static vmux_ecs::profile::ProfileDirectories>,
    connections: PersistedConnections<'w, 's>,
    changed: ChangedPersistedConnections<'w, 's>,
    pending: Query<'w, 's, (), With<ConnectionPersistTask>>,
    removed: RemovedComponents<'w, 's, Connection>,
    validated: RemovedComponents<'w, 's, ConnectionValidationPending>,
    persistence: ResMut<'w, ConnectionPersistence>,
    runtime: Res<'w, BlockingRuntime>,
    wake: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
}

fn persist(mut context: PersistContext, mut commands: Commands) {
    if !context.changed.is_empty()
        || context.removed.read().count() != 0
        || context.validated.read().count() != 0
    {
        context.persistence.dirty = true;
    }
    if context.persistence.blocked
        || !context.persistence.dirty
        || !context.pending.is_empty()
        || context
            .persistence
            .retry_at
            .is_some_and(|retry_at| retry_at > Instant::now())
    {
        return;
    }
    let Ok(profile) = context.profiles.single() else {
        return;
    };
    let file = ConnectionFile {
        version: 1,
        connections: context
            .connections
            .iter()
            .map(
                |(id, provider, name, account, access, pending, state, disconnect)| {
                    ConnectionRecord {
                        id: id.as_str().to_string(),
                        provider: provider.0.clone(),
                        name: pending
                            .map_or_else(|| name.0.clone(), |pending| pending.name.clone()),
                        account_external_id: pending.map_or_else(
                            || account.map(|account| account.0.clone()).unwrap_or_default(),
                            |pending| pending.account_external_id.clone(),
                        ),
                        scopes: pending.map_or_else(
                            || {
                                access
                                    .into_iter()
                                    .flat_map(|access| &access.0)
                                    .map(|scope| scope.as_str().to_string())
                                    .collect()
                            },
                            |pending| pending.scopes.clone(),
                        ),
                        disconnected: disconnect.is_some()
                            || *state == ConnectionState::Disconnected,
                    }
                },
            )
            .collect(),
    };
    let path = profile.0.profile().join("connections.ron");
    let wake = context.wake.as_deref().map(|wake| (**wake).clone());
    let Some(task) = context.runtime.spawn(wake, move || {
        let source = ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::default())
            .map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(path, source.as_bytes()).map_err(|error| error.to_string())
    }) else {
        context.persistence.retry_at = Some(Instant::now() + Duration::from_secs(1));
        let error = "Connection persistence queue is full".to_string();
        if context.persistence.last_error.as_deref() != Some(error.as_str()) {
            commands.spawn((
                Operation,
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                OperationKind::new("auth.connection.persist").expect("valid operation metadata"),
                OperationState::Failed,
                OperationError::new(error.clone()).expect("valid operation metadata"),
            ));
        }
        context.persistence.last_error = Some(error);
        return;
    };
    context.persistence.dirty = false;
    commands.spawn((
        Operation,
        vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
        OperationKind::new("auth.connection.persist").expect("valid operation metadata"),
        OperationState::Running,
        ConnectionPersistTask(task),
    ));
}

fn finish_persist(
    mut tasks: Query<(Entity, &mut ConnectionPersistTask)>,
    mut persistence: ResMut<ConnectionPersistence>,
    mut commands: Commands,
) {
    for (operation, mut task) in &mut tasks {
        let Some(result) = task.0.poll() else {
            continue;
        };
        commands.entity(operation).remove::<ConnectionPersistTask>();
        let result = match result {
            Ok(result) => result,
            Err(_) => Err("Connection persistence panicked".to_string()),
        };
        match result {
            Ok(()) => {
                persistence.retry_at = None;
                persistence.last_error = None;
                commands.entity(operation).insert(OperationState::Succeeded);
            }
            Err(error) => {
                persistence.dirty = true;
                persistence.retry_at = Some(Instant::now() + Duration::from_secs(1));
                if persistence.last_error.as_deref() == Some(error.as_str()) {
                    commands.entity(operation).despawn();
                } else {
                    commands.entity(operation).insert((
                        OperationState::Failed,
                        OperationError::new(error.clone()).expect("valid operation metadata"),
                    ));
                }
                persistence.last_error = Some(error);
            }
        }
    }
}

fn connect(
    mut requests: MessageReader<ConnectRequest>,
    providers: Query<(&ProviderKey, &AuthProviderDefinition)>,
    credentials: Query<&CredentialStore>,
    pending: Query<&ConnectTask>,
    runtime: Res<AuthRuntime>,
    wake: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok(credentials) = credentials.single() else {
        return;
    };
    let mut active = pending
        .iter()
        .map(|task| task.provider.clone())
        .collect::<HashSet<_>>();
    for request in requests.read() {
        if !active.insert(request.provider.clone()) {
            commands.spawn((
                Operation,
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                OperationKind::new("auth.connect").expect("valid operation metadata"),
                OperationState::Failed,
                OperationError::new(format!(
                    "OAuth authorization is already running for {}",
                    request.provider.0
                ))
                .expect("valid operation metadata"),
            ));
            continue;
        }
        if !credentials.storage.available() {
            commands.spawn((
                Operation,
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                OperationKind::new("auth.connect").expect("valid operation metadata"),
                OperationState::Failed,
                OperationError::new(
                    "Encrypted OAuth credential storage is unavailable on this platform",
                )
                .expect("valid operation metadata"),
            ));
            continue;
        }
        let provider = providers
            .iter()
            .find_map(|(key, definition)| (key == &request.provider).then(|| definition.0.clone()));
        let Some(provider) = provider else {
            commands.spawn((
                Operation,
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                OperationKind::new("auth.connect").expect("valid operation metadata"),
                OperationState::Failed,
                OperationError::new(format!("Unknown auth provider: {}", request.provider.0))
                    .expect("valid operation metadata"),
            ));
            continue;
        };
        let id = Id::<Connection>::new(uuid::Uuid::new_v4().to_string());
        let connection = commands
            .spawn((
                Connection,
                id.clone(),
                request.provider.clone(),
                ConnectionName(provider.name.clone()),
                ConnectionState::Connecting,
                CanAccess::default(),
            ))
            .id();
        let operation = commands
            .spawn((
                Operation,
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                OperationKind::new("auth.connect").expect("valid operation metadata"),
                OperationState::Running,
            ))
            .id();
        let access = credentials.access(&request.provider, &id);
        let Ok((credential_revision, ())) = access.read_versioned(|| Ok(())) else {
            commands.entity(operation).insert((
                OperationState::Failed,
                OperationError::new("OAuth credential store is unavailable")
                    .expect("valid operation metadata"),
            ));
            credentials.remove_access(&request.provider, &id);
            commands.entity(connection).despawn();
            continue;
        };
        let wake = wake.as_deref().map(|wake| (**wake).clone());
        match ConnectTask::spawn(
            &runtime,
            ConnectTaskInput {
                connection,
                id: id.clone(),
                provider: request.provider.clone(),
                definition: provider,
                credential_revision,
                credentials: credentials.clone(),
                wake,
            },
        ) {
            Ok(task) => {
                commands.entity(operation).insert(task);
            }
            Err(error) => {
                commands.entity(operation).insert((
                    OperationState::Failed,
                    OperationError::new(error.clone()).expect("valid operation metadata"),
                ));
                credentials.remove_access(&request.provider, &id);
                commands.entity(connection).despawn();
            }
        }
    }
}

fn update_operations(
    mut operations: Query<(Entity, &mut ConnectTask)>,
    mut states: Query<(&mut ConnectionState, &ConnectionName)>,
    credentials: Query<&CredentialStore>,
    mut commands: Commands,
) {
    let store = credentials.single().ok();
    for (operation, mut pending) in &mut operations {
        let canceled = pending.cancellation.is_canceled();
        if !canceled {
            while let Ok(progress) = pending.progress.get_mut().try_recv() {
                let Ok((mut state, _)) = states.get_mut(pending.connection) else {
                    continue;
                };
                match progress {
                    AuthProgress::AwaitingUser {
                        verification_uri,
                        user_code,
                        expires_at,
                    } => {
                        *state = ConnectionState::AwaitingUser {
                            verification_uri,
                            user_code,
                            expires_at,
                        };
                    }
                }
            }
        }
        let result = match pending.result.get_mut().try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("OAuth authorization task ended without a result".to_string())
            }
        };
        commands.entity(operation).remove::<ConnectTask>();
        if canceled {
            match result {
                Ok(ConnectOutcome::DisconnectPending(error)) | Err(error) => {
                    commands.entity(operation).insert((
                        OperationState::Failed,
                        OperationError::new(error).expect("valid operation metadata"),
                    ));
                }
                Ok(_) => {
                    commands.entity(operation).insert(OperationState::Canceled);
                }
            }
            continue;
        }
        match result {
            Ok(ConnectOutcome::Connected) => {
                commands.entity(operation).insert(OperationState::Succeeded);
                let name = states
                    .get(pending.connection)
                    .map(|(_, name)| name.0.clone())
                    .unwrap_or_default();
                commands.entity(pending.connection).insert((
                    ConnectionState::Connected,
                    ConnectionValidationPending {
                        name,
                        account_external_id: String::new(),
                        scopes: Vec::new(),
                    },
                ));
                commands.trigger(Connected {
                    entity: pending.connection,
                    id: pending.id.clone(),
                    provider: pending.provider.clone(),
                });
            }
            Ok(ConnectOutcome::Canceled) => {
                commands.entity(operation).insert(OperationState::Canceled);
                if let Some(store) = store {
                    store.remove_access(&pending.provider, &pending.id);
                }
                commands.entity(pending.connection).despawn();
            }
            Ok(ConnectOutcome::DisconnectPending(error)) => {
                commands.entity(operation).insert((
                    OperationState::Failed,
                    OperationError::new(error).expect("valid operation metadata"),
                ));
                commands.entity(pending.connection).insert((
                    ConnectionState::Disconnected,
                    CanAccess::default(),
                    DisconnectRequested,
                    DisconnectRetryAt(Instant::now() + Duration::from_secs(1)),
                ));
            }
            Err(error) => {
                commands.entity(operation).insert((
                    OperationState::Failed,
                    OperationError::new(error.clone()).expect("valid operation metadata"),
                ));
                if let Some(store) = store {
                    store.remove_access(&pending.provider, &pending.id);
                }
                commands.entity(pending.connection).despawn();
            }
        }
    }
}

type DisconnectConnections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id<Connection>,
        &'static ProviderKey,
        &'static mut ConnectionState,
    ),
>;

#[derive(SystemParam)]
struct DisconnectContext<'w, 's> {
    requests: MessageReader<'w, 's, DisconnectRequest>,
    connections: DisconnectConnections<'w, 's>,
    providers: Query<'w, 's, (&'static ProviderKey, &'static AuthProviderDefinition)>,
    credentials: Query<'w, 's, &'static CredentialStore>,
    pending: Query<'w, 's, &'static ConnectTask>,
    disconnecting: Query<'w, 's, &'static DisconnectTask>,
    runtime: Res<'w, AuthRuntime>,
    wake: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
}

fn disconnect(context: DisconnectContext, mut commands: Commands) {
    let DisconnectContext {
        mut requests,
        mut connections,
        providers,
        credentials,
        pending,
        disconnecting,
        runtime,
        wake,
    } = context;
    let Ok(credentials) = credentials.single() else {
        return;
    };
    let mut started = HashSet::new();
    for request in requests.read() {
        let Some((entity, id, provider, mut state)) = connections
            .iter_mut()
            .find(|(_, id, _, _)| *id == &request.connection)
        else {
            continue;
        };
        if !started.insert(entity) || disconnecting.iter().any(|task| task.connection == entity) {
            continue;
        }
        let definition = providers
            .iter()
            .find_map(|(key, definition)| (key == provider).then(|| definition.0.clone()));
        let access = credentials.access(provider, id);
        access.invalidate();
        *state = ConnectionState::Disconnected;
        commands
            .entity(entity)
            .insert((CanAccess::default(), DisconnectRequested));
        let mut connecting = false;
        for task in &pending {
            if task.connection == entity {
                task.cancel();
                connecting = true;
            }
        }
        if connecting {
            commands
                .entity(entity)
                .insert(DisconnectRetryAt(Instant::now()));
            continue;
        }
        let storage = credentials.storage.clone();
        let id = id.clone();
        let provider = provider.clone();
        let worker_id = id.clone();
        let worker_provider = provider.clone();
        let worker_access = access.clone();
        let wake = wake.as_deref().map(|wake| (**wake).clone());
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let (start_sender, start_receiver) = mpsc::sync_channel(1);
        let queued = runtime.spawn(move || {
            if start_receiver.recv().is_err() {
                return;
            }
            let result = (|| {
                let revocation = match definition {
                    Some(definition) => {
                        let loaded = storage
                            .load_retrying(&worker_provider.0, worker_id.as_str(), 3)
                            .map_err(|error| {
                                format!(
                                    "OAuth credentials could not be loaded for revocation: {error}"
                                )
                            })?;
                        match loaded {
                            Some(credentials) => {
                                AuthFlow::revoke_retrying(&definition, &credentials, 3)?
                            }
                            None => Revocation::Revoked,
                        }
                    }
                    None => Revocation::Unsupported,
                };
                worker_access
                    .cleanup(|| storage.remove(&worker_provider.0, worker_id.as_str()))
                    .map_err(|error| format!("OAuth credentials could not be removed: {error}"))?;
                Ok(revocation)
            })();
            let _ = result_sender.send(result);
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        });
        if let Err(error) = queued {
            commands.spawn((
                Operation,
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                OperationKind::new("auth.disconnect").expect("valid operation metadata"),
                OperationState::Failed,
                OperationError::new(error.clone()).expect("valid operation metadata"),
            ));
            commands.entity(entity).insert((
                ConnectionState::Disconnected,
                DisconnectRequested,
                DisconnectRetryAt(Instant::now() + Duration::from_secs(1)),
            ));
            continue;
        }
        let _ = start_sender.send(());
        commands.spawn((
            Operation,
            vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
            vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
            OperationKind::new("auth.disconnect").expect("valid operation metadata"),
            OperationState::Running,
            DisconnectTask {
                connection: entity,
                id,
                provider,
                result: Mutex::new(result_receiver),
            },
        ));
    }
}

fn finish_disconnect(
    mut tasks: Query<(Entity, &mut DisconnectTask)>,
    credentials: Query<&CredentialStore>,
    mut commands: Commands,
) {
    let store = credentials.single().ok();
    for (operation, mut task) in &mut tasks {
        let result = match task.result.get_mut().try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Connection disconnect task ended without a result".to_string())
            }
        };
        commands.entity(operation).remove::<DisconnectTask>();
        match result {
            Ok(Revocation::Revoked) => {
                if let Some(store) = store {
                    store.remove_access(&task.provider, &task.id);
                }
                commands.trigger(Disconnected {
                    entity: task.connection,
                    id: task.id.clone(),
                    provider: task.provider.clone(),
                });
                commands.entity(task.connection).despawn();
                commands.entity(operation).insert(OperationState::Succeeded);
            }
            Ok(Revocation::Unsupported) => {
                if let Some(store) = store {
                    store.remove_access(&task.provider, &task.id);
                }
                commands.spawn((
                    Operation,
                    vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                    vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                    OperationKind::new("auth.disconnect.revoke").expect("valid operation metadata"),
                    OperationState::Canceled,
                ));
                commands.trigger(Disconnected {
                    entity: task.connection,
                    id: task.id.clone(),
                    provider: task.provider.clone(),
                });
                commands.entity(task.connection).despawn();
                commands.entity(operation).insert(OperationState::Succeeded);
            }
            Err(error) => {
                commands.entity(operation).insert((
                    OperationState::Failed,
                    OperationError::new(error.clone()).expect("valid operation metadata"),
                ));
                commands.entity(task.connection).insert((
                    ConnectionState::Disconnected,
                    DisconnectRequested,
                    DisconnectRetryAt(Instant::now() + Duration::from_secs(1)),
                ));
            }
        }
    }
}

fn retry_disconnects(
    pending: Query<&DisconnectTask>,
    connecting: Query<&ConnectTask>,
    retries: Query<(Entity, &Id<Connection>, &DisconnectRetryAt), With<DisconnectRequested>>,
    mut requests: MessageWriter<DisconnectRequest>,
    mut commands: Commands,
) {
    let now = Instant::now();
    for (entity, id, retry) in &retries {
        if retry.0 > now
            || pending.iter().any(|task| task.connection == entity)
            || connecting.iter().any(|task| task.connection == entity)
        {
            continue;
        }
        requests.write(DisconnectRequest {
            connection: id.clone(),
        });
        commands.entity(entity).remove::<DisconnectRetryAt>();
    }
}
