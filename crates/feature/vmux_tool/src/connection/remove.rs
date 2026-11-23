use super::auth::{Credentials, Source};
use super::{McpCatalogServer, McpCompletion, PendingMcpOperation};
use crate::storage::{ReadRequest, ReadResult, StoreLease, WriteRequest, WriteResult};
use crate::{ToolStore, ToolsManifest};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use vmux_ecs::profile::McpCredentials;
use vmux_ecs::{EntityTarget, OperationCompleted};
use vmux_profile::mcp_credentials::{McpCredentialTransaction, McpOauthCredentials};

pub(super) struct RemovePlugin;

impl Plugin for RemovePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, begin.before(RemoveSet))
            .add_systems(
                PostUpdate,
                (
                    poll::<StoreLease>,
                    poll::<CredentialLease>,
                    poll::<Credential>,
                    poll::<CredentialRemoved>,
                    poll::<CredentialRestored>,
                ),
            )
            .add_systems(
                Update,
                (
                    store_lock,
                    credential_lock,
                    read_manifest,
                    read_credential,
                    prepare,
                    write_manifest,
                    delete,
                    rollback,
                    finish,
                )
                    .chain()
                    .in_set(RemoveSet),
            );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct RemoveSet;
#[derive(Component)]
pub(super) struct Active;
#[derive(Component)]
struct PhaseState(Phase);
enum Phase {
    StoreLock,
    CredentialLock,
    Manifest,
    Credential,
    Prepare,
    WriteManifest,
    RemoveCredential,
    RestoreManifest,
    RestoreCredential,
    Complete,
}
#[derive(Component)]
struct Busy;
#[derive(Component)]
struct IoTask<T: Component>(Task<Result<T, String>>);
#[derive(Component)]
struct Failed<T: Component>(String, std::marker::PhantomData<fn() -> T>);
#[derive(Component)]
struct Failure(String);
#[derive(Component)]
struct CredentialLease {
    _lease: McpCredentialTransaction,
}
#[derive(Component)]
struct Manifest(Option<ToolsManifest>);
#[derive(Component)]
struct Credential(Option<McpOauthCredentials>);
#[derive(Component)]
struct CredentialRemoved;
#[derive(Component)]
struct CredentialRestored;

fn poll<T: Component>(
    mut tasks: Query<(Entity, &mut IoTask<T>)>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    let waker = wake
        .as_deref()
        .map(|wake| &wake.0)
        .unwrap_or(std::task::Waker::noop());
    let mut context = Context::from_waker(waker);
    for (entity, mut task) in &mut tasks {
        let Poll::Ready(result) = Pin::new(&mut task.0).poll(&mut context) else {
            continue;
        };
        waker.wake_by_ref();
        commands.entity(entity).remove::<(IoTask<T>, Busy)>();
        match result {
            Ok(result) => {
                commands.entity(entity).insert(result);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Failed::<T>(error, std::marker::PhantomData));
            }
        }
    }
}

type BeginRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PendingMcpOperation,
        &'static EntityTarget<McpCatalogServer>,
    ),
    Without<Active>,
>;

fn begin(
    requests: BeginRequests<'_, '_>,
    servers: Query<&McpCatalogServer>,
    credentials: Query<(
        Entity,
        &McpCredentials,
        Option<&vmux_ecs::profile::ProfileDirectories>,
    )>,
    mut commands: Commands,
) {
    for (entity, request, server) in &requests {
        if !request.disconnect {
            continue;
        }
        let current = credentials.single().ok();
        let selected = servers.get(server.entity()).ok();
        let error = if current.is_none_or(|(owner, _, _)| request.owner != owner) {
            Some("MCP source Profile changed")
        } else if selected.is_none() {
            Some("MCP server is no longer available")
        } else {
            None
        };
        if let Some(error) = error {
            commands
                .entity(entity)
                .remove::<PendingMcpOperation>()
                .insert(McpCompletion {
                    id: request.id.clone(),
                    disconnect: true,
                    success: false,
                    message: error.into(),
                });
            commands.trigger(OperationCompleted {
                entity,
                result: Err(error.into()),
            });
            continue;
        }
        let (owner, credentials, directories) = current.unwrap();
        let server = selected.unwrap();
        let store = match directories {
            Some(paths) => ToolStore::new(paths.0.config(), paths.0.home()),
            None => ToolStore::current(),
        };
        commands
            .entity(entity)
            .remove::<PendingMcpOperation>()
            .insert((
                Active,
                Source {
                    generation: request.generation,
                    id: request.id.clone(),
                    owner,
                    disconnect: true,
                },
                Credentials(credentials.clone()),
                server.clone(),
                store,
                PhaseState(Phase::StoreLock),
            ));
    }
}

type StoreLockRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static ToolStore,
        Option<&'static StoreLease>,
        Option<&'static Failed<StoreLease>>,
    ),
    (With<Active>, Without<Busy>),
>;

fn store_lock(requests: StoreLockRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, store, done, error) in &requests {
        if !matches!(phase.0, Phase::StoreLock) {
            continue;
        }
        if let Some(error) = error {
            commands
                .entity(entity)
                .insert((Failure(error.0.clone()), PhaseState(Phase::Complete)));
        } else if done.is_some() {
            commands
                .entity(entity)
                .insert(PhaseState(Phase::CredentialLock));
        } else {
            let store = store.clone();
            commands.entity(entity).insert((
                Busy,
                IoTask(IoTaskPool::get().spawn(async move {
                    Ok(StoreLease {
                        _lease: store.lease_async().await,
                    })
                })),
            ));
        }
    }
}

type CredentialLockRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Credentials,
        Option<&'static CredentialLease>,
        Option<&'static Failed<CredentialLease>>,
    ),
    (With<Active>, Without<Busy>),
>;

fn credential_lock(requests: CredentialLockRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, credentials, done, error) in &requests {
        if !matches!(phase.0, Phase::CredentialLock) {
            continue;
        }
        if let Some(error) = error {
            commands
                .entity(entity)
                .insert((Failure(error.0.clone()), PhaseState(Phase::Complete)));
        } else if done.is_some() {
            commands.entity(entity).insert(PhaseState(Phase::Manifest));
        } else {
            let access = credentials.0.access.clone();
            commands.entity(entity).insert((
                Busy,
                IoTask(IoTaskPool::get().spawn(async move {
                    access
                        .transaction()
                        .await
                        .map(|lease| CredentialLease { _lease: lease })
                })),
            ));
        }
    }
}

fn read_manifest(
    requests: Query<
        (
            Entity,
            &PhaseState,
            Option<&ReadResult>,
            Option<&ReadRequest>,
        ),
        (With<Active>, Without<Busy>),
    >,
    mut commands: Commands,
) {
    for (entity, phase, result, pending) in &requests {
        if !matches!(phase.0, Phase::Manifest) {
            continue;
        }
        if let Some(result) = result {
            commands.entity(entity).remove::<ReadResult>();
            match &result.0 {
                Ok(manifest) => {
                    commands.entity(entity).insert((
                        Manifest(Some(manifest.clone())),
                        PhaseState(Phase::Credential),
                    ));
                }
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert((Failure(error.clone()), PhaseState(Phase::Complete)));
                }
            }
        } else if pending.is_none() {
            commands
                .entity(entity)
                .insert(ReadRequest { retain_lease: true });
        }
    }
}

type ReadCredentialRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Source,
        &'static Credentials,
        Option<&'static Credential>,
        Option<&'static Failed<Credential>>,
    ),
    (With<Active>, Without<Busy>),
>;

fn read_credential(requests: ReadCredentialRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, source, credentials, done, error) in &requests {
        if !matches!(phase.0, Phase::Credential) {
            continue;
        }
        if let Some(error) = error {
            commands
                .entity(entity)
                .insert((Failure(error.0.clone()), PhaseState(Phase::Complete)));
        } else if done.is_some() {
            commands.entity(entity).insert(PhaseState(Phase::Prepare));
        } else {
            let id = source.id.clone();
            let storage = credentials.0.storage.clone();
            commands.entity(entity).insert((
                Busy,
                IoTask(IoTaskPool::get().spawn(async move { storage.load(&id).map(Credential) })),
            ));
        }
    }
}

type PrepareRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Source,
        &'static McpCatalogServer,
        &'static Manifest,
    ),
    (With<Active>, Without<Busy>),
>;

fn prepare(
    requests: PrepareRequests<'_, '_>,
    current: Query<Entity, With<McpCredentials>>,
    mut commands: Commands,
) {
    for (entity, phase, source, server, manifest) in &requests {
        if !matches!(phase.0, Phase::Prepare) {
            continue;
        }
        let configured = manifest.0.as_ref().unwrap().mcp.servers.get(&source.id);
        let error = if current.single().ok() != Some(source.owner) {
            Some("MCP source Profile changed".into())
        } else if configured.is_none() {
            Some(format!("MCP server is not configured: {}", source.id))
        } else if !server.owns(configured.unwrap()) {
            Some(format!(
                "MCP server ID is already managed by tools.toml: {}",
                source.id
            ))
        } else {
            None
        };
        if let Some(error) = error {
            commands
                .entity(entity)
                .insert((Failure(error), PhaseState(Phase::Complete)));
        } else {
            commands
                .entity(entity)
                .insert(PhaseState(Phase::WriteManifest));
        }
    }
}

fn write_manifest(
    requests: Query<
        (
            Entity,
            &PhaseState,
            &Source,
            &Manifest,
            Option<&WriteResult>,
            Option<&WriteRequest>,
        ),
        (With<Active>, Without<Busy>),
    >,
    mut commands: Commands,
) {
    for (entity, phase, source, original, result, pending) in &requests {
        if !matches!(phase.0, Phase::WriteManifest) {
            continue;
        }
        if let Some(result) = result {
            commands.entity(entity).remove::<WriteResult>();
            match &result.0 {
                Ok(()) => {
                    commands
                        .entity(entity)
                        .insert(PhaseState(Phase::RemoveCredential));
                }
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert((Failure(error.clone()), PhaseState(Phase::RestoreManifest)));
                }
            }
        } else if pending.is_none() {
            let mut manifest = original.0.as_ref().unwrap().clone();
            manifest.mcp.servers.remove(&source.id);
            commands.entity(entity).insert(WriteRequest {
                manifest,
                retain_lease: true,
            });
        }
    }
}

type RemoveCredentialRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Source,
        &'static Credentials,
        Option<&'static CredentialRemoved>,
        Option<&'static Failed<CredentialRemoved>>,
    ),
    (With<Active>, Without<Busy>),
>;

fn delete(requests: RemoveCredentialRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, source, credentials, done, error) in &requests {
        if !matches!(phase.0, Phase::RemoveCredential) {
            continue;
        }
        if let Some(error) = error {
            commands
                .entity(entity)
                .insert((Failure(error.0.clone()), PhaseState(Phase::RestoreManifest)));
        } else if done.is_some() {
            commands.entity(entity).insert(PhaseState(Phase::Complete));
        } else {
            let id = source.id.clone();
            let storage = credentials.0.storage.clone();
            commands.entity(entity).insert((
                Busy,
                IoTask(
                    IoTaskPool::get()
                        .spawn(async move { storage.remove(&id).map(|_| CredentialRemoved) }),
                ),
            ));
        }
    }
}

fn rollback(
    requests: Query<
        (
            Entity,
            &PhaseState,
            &Source,
            &Credentials,
            &Manifest,
            &Credential,
            Option<&WriteResult>,
            Option<&WriteRequest>,
            Option<&CredentialRestored>,
            Option<&Failed<CredentialRestored>>,
        ),
        (With<Active>, Without<Busy>),
    >,
    mut failures: Query<&mut Failure>,
    mut commands: Commands,
) {
    for (
        entity,
        phase,
        source,
        credentials,
        original,
        old,
        manifest,
        pending,
        credential_done,
        credential_error,
    ) in &requests
    {
        if matches!(phase.0, Phase::RestoreManifest) {
            if let Some(result) = manifest {
                commands.entity(entity).remove::<WriteResult>();
                if let Err(error) = &result.0
                    && let Ok(mut failure) = failures.get_mut(entity)
                {
                    failure
                        .0
                        .push_str(&format!("; manifest rollback failed: {error}"));
                }
                commands
                    .entity(entity)
                    .insert(PhaseState(Phase::RestoreCredential));
            } else if pending.is_none() {
                commands.entity(entity).insert(WriteRequest {
                    manifest: original.0.as_ref().unwrap().clone(),
                    retain_lease: true,
                });
            }
        } else if matches!(phase.0, Phase::RestoreCredential) {
            if credential_done.is_some() || credential_error.is_some() {
                if let Some(error) = credential_error
                    && let Ok(mut failure) = failures.get_mut(entity)
                {
                    failure
                        .0
                        .push_str(&format!("; credential rollback failed: {}", error.0));
                }
                commands.entity(entity).insert(PhaseState(Phase::Complete));
                continue;
            }
            let id = source.id.clone();
            let storage = credentials.0.storage.clone();
            let old = old.0.clone();
            commands.entity(entity).insert((
                Busy,
                IoTask(IoTaskPool::get().spawn(async move {
                    let result = match old {
                        Some(old) => storage.store(&id, &old),
                        None => storage.remove(&id),
                    };
                    result.map(|_| CredentialRestored)
                })),
            ));
        }
    }
}

type FinishRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Source,
        Option<&'static Failure>,
    ),
    (With<Active>, Without<Busy>),
>;

fn finish(requests: FinishRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, source, failure) in &requests {
        if !matches!(phase.0, Phase::Complete) {
            continue;
        }
        let message = failure.map(|failure| failure.0.clone()).unwrap_or_default();
        let success = failure.is_none();
        commands
            .entity(entity)
            .insert(McpCompletion {
                id: source.id.clone(),
                disconnect: true,
                success,
                message: message.clone(),
            })
            .remove::<(Active, PhaseState, ToolStore, Credentials, McpCatalogServer)>();
        commands
            .entity(entity)
            .remove::<(Manifest, Credential, StoreLease, CredentialLease)>();
        commands.trigger(OperationCompleted {
            entity,
            result: if success { Ok(()) } else { Err(message) },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::{TaskPoolBuilder, futures_lite::future};
    use std::time::{Duration, Instant};
    use vmux_ecs::profile::mcp_credentials::McpCredentialAccess;
    use vmux_ecs::{Operation, OperationError, OperationRetained, OperationState};

    #[test]
    fn credential_removal_failure_restores_configuration_and_reports_rollback_failure() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        let server = McpCatalogServer {
            setup_url: None,
            client_id: None,
            client_id_env: None,
            account_endpoint: None,
            id: "linear".into(),
            url: "https://mcp.linear.app/mcp".into(),
            scopes: Vec::new(),
        };
        let mut original = ToolsManifest::default();
        original
            .mcp
            .servers
            .insert(server.id.clone(), server.manifest());
        ToolsManifest::default()
            .write_to(&store.manifest_path())
            .unwrap();
        let access = McpCredentialAccess::default();
        let transaction = future::block_on(access.transaction()).unwrap();
        let store_lease = future::block_on(store.lease_async());
        let credentials = McpCredentials {
            access: access.clone(),
            storage: vmux_profile::mcp_credentials::McpCredentialStorage::at(
                vmux_profile::Profile::named("native-disconnect-contract"),
                vmux_profile::ProfilePaths::current(),
            ),
        };
        let mut app = App::new();
        app.add_plugins(super::super::McpConnectionPlugin);
        let owner = app.world_mut().spawn_empty().id();
        let catalog = app.world_mut().spawn(server).id();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.disconnect").unwrap().running(),
                OperationRetained,
                EntityTarget::<McpCatalogServer>::new(catalog),
                super::super::McpConnectionOperation,
                Active,
                PhaseState(Phase::RemoveCredential),
                Source {
                    generation: 1,
                    id: "linear".into(),
                    owner,
                    disconnect: true,
                },
                store.clone(),
                Credentials(credentials),
                Manifest(Some(original.clone())),
                Credential(None),
                StoreLease {
                    _lease: store_lease,
                },
                CredentialLease {
                    _lease: transaction,
                },
                Failed::<CredentialRemoved>(
                    "mock removal failure".into(),
                    std::marker::PhantomData,
                ),
                Failed::<CredentialRestored>(
                    "mock credential rollback failure".into(),
                    std::marker::PhantomData,
                ),
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.world().get::<Active>(operation).is_some() {
            app.update();
            assert!(
                Instant::now() < deadline,
                "disconnect rollback did not drain"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            ToolsManifest::read(&store.manifest_path())
                .unwrap()
                .mcp
                .servers,
            original.mcp.servers
        );
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert_eq!(
            app.world()
                .get::<OperationError>(operation)
                .unwrap()
                .message(),
            "mock removal failure; credential rollback failed: mock credential rollback failure"
        );
        assert!(app.world().get::<StoreLease>(operation).is_none());
        assert!(app.world().get::<CredentialLease>(operation).is_none());
        assert!(app.world().get::<OperationRetained>(operation).is_none());
        assert!(access.stable_revision().unwrap().is_multiple_of(2));
    }

    #[test]
    fn begin_uses_profile_directories_instead_of_global_tool_store() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
        let directory = tempfile::tempdir().unwrap();
        let profile = vmux_profile::Profile::named("isolated-mcp-profile");
        let paths = vmux_profile::ProfilePaths::at(profile.clone(), directory.path().to_path_buf());
        let expected_store = ToolStore::new(paths.config(), paths.home());
        let credentials = McpCredentials {
            access: McpCredentialAccess::default(),
            storage: vmux_profile::mcp_credentials::McpCredentialStorage::at(
                profile,
                paths.clone(),
            ),
        };
        let mut app = App::new();
        app.add_plugins(super::super::McpConnectionPlugin);
        let owner = app
            .world_mut()
            .spawn((credentials, vmux_ecs::profile::ProfileDirectories(paths)))
            .id();
        let catalog = app
            .world_mut()
            .spawn(McpCatalogServer {
                setup_url: None,
                client_id: None,
                client_id_env: None,
                account_endpoint: None,
                id: "linear".into(),
                url: "https://mcp.linear.app/mcp".into(),
                scopes: Vec::new(),
            })
            .id();
        let connect = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.connect").unwrap().running(),
                EntityTarget::<McpCatalogServer>::new(catalog),
                PendingMcpOperation {
                    generation: 1,
                    owner,
                    id: "linear".into(),
                    disconnect: false,
                },
            ))
            .id();
        let disconnect = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.disconnect").unwrap().running(),
                EntityTarget::<McpCatalogServer>::new(catalog),
                PendingMcpOperation {
                    generation: 2,
                    owner,
                    id: "linear".into(),
                    disconnect: true,
                },
            ))
            .id();

        app.update();

        assert_eq!(app.world().get::<ToolStore>(connect), Some(&expected_store));
        assert_eq!(
            app.world().get::<ToolStore>(disconnect),
            Some(&expected_store)
        );
    }
}
