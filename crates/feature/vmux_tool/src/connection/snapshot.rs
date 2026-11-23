use super::{
    McpCatalogServer, McpCompletion, McpConfiguredServer, McpObservationVersion, McpPageState,
    McpServerFacts, PendingMcpSnapshot, auth,
};
use crate::storage::{ReadRequest, ReadResult, StoreLease};
use crate::{ToolStore, ToolsManifest};
use bevy::ecs::change_detection::Tick;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use vmux_ecs::profile::McpCredentials;
use vmux_ecs::{
    EntityTarget, Operation, OperationCompleted, OperationRetained, OperationState, OperationTarget,
};
use vmux_profile::mcp_credentials::{
    McpCredentialStorage, McpCredentialTransaction, McpOauthCredentials,
};
use zeroize::Zeroize;

pub(super) struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                invalidate,
                start,
                cancel,
                manifest,
                lease,
                schedule,
                poll_lease,
                poll_credentials,
                drain,
                materialize,
                finish,
                wake,
            )
                .chain()
                .in_set(SnapshotSet)
                .after(auth::AuthSet)
                .after(super::remove::RemoveSet),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct SnapshotSet;

#[derive(Component)]
pub(super) struct SnapshotContext {
    pub(super) generation: u64,
    pub(super) sequence: u64,
    pub(super) owner: Entity,
    source_tick: Tick,
    pub(super) result: Option<McpCompletion>,
}

#[derive(Component)]
struct SnapshotManifest(Result<ToolsManifest, String>);

#[derive(Component)]
struct Reading;

#[derive(Component)]
struct Ready;

#[derive(Component)]
struct Refused(String);

#[derive(Component)]
struct CredentialLease {
    _lease: McpCredentialTransaction,
}

#[derive(Component)]
struct CredentialLeaseTask(Task<Result<CredentialLease, String>>);

#[derive(Component)]
struct CredentialReadRequest {
    id: String,
    source_tick: Tick,
    storage: McpCredentialStorage,
}

impl CredentialReadRequest {
    fn load(&self) -> Result<Option<SecretHandle>, String> {
        self.storage
            .load(&self.id)
            .map(|value| value.map(SecretHandle))
    }
}

struct SecretHandle(McpOauthCredentials);

impl Drop for SecretHandle {
    fn drop(&mut self) {
        self.0.client_secret.zeroize();
        self.0.access_token.zeroize();
        self.0.refresh_token.zeroize();
        self.0.client_id.zeroize();
        self.0.token_endpoint.zeroize();
        self.0.scope.zeroize();
        self.0.resource.zeroize();
    }
}

#[derive(Component)]
struct CredentialReadTask(Task<Result<Option<SecretHandle>, String>>);

#[derive(Component)]
struct CredentialReadResult(Result<Option<SecretHandle>, String>);

#[derive(Component)]
struct SnapshotServerFacts {
    source_tick: Tick,
    facts: McpServerFacts,
}

fn invalidate(
    credentials: Query<(Entity, Ref<McpCredentials>)>,
    mut version: ResMut<McpObservationVersion>,
    mut states: Query<&mut McpPageState>,
) {
    if !version.valid {
        return;
    }
    if credentials.single().is_ok_and(|(owner, source)| {
        Some(owner) == version.owner && Some(source.last_changed()) == version.source_tick
    }) {
        return;
    }
    version.valid = false;
    for mut state in &mut states {
        state.loaded = false;
    }
}

fn start(
    requests: Query<(Entity, &PendingMcpSnapshot)>,
    credentials: Query<(
        Entity,
        Ref<McpCredentials>,
        Option<&vmux_ecs::profile::ProfileDirectories>,
    )>,
    mut version: ResMut<McpObservationVersion>,
    mut states: Query<&mut McpPageState>,
    mut commands: Commands,
) {
    let Ok((owner, credentials, directories)) = credentials.single() else {
        version.valid = false;
        version.owner = None;
        version.source_tick = None;
        for (target, request) in &requests {
            if let Ok(mut state) = states.get_mut(target) {
                state.loading = false;
                state.pending = false;
                state.loaded = false;
                state.owner = None;
            }
            commands
                .entity(target)
                .remove::<PendingMcpSnapshot>()
                .insert(request.result.clone().unwrap_or(McpCompletion {
                    id: String::new(),
                    disconnect: false,
                    success: false,
                    message: "MCP source Profile is unavailable".into(),
                }));
        }
        return;
    };
    for (target, request) in &requests {
        commands.entity(target).remove::<PendingMcpSnapshot>();
        let Ok(mut state) = states.get_mut(target) else {
            continue;
        };
        if state.generation != request.generation {
            continue;
        }
        state.owner = Some(owner);
        version.next = version.next.wrapping_add(1).max(1);
        let store = match directories {
            Some(paths) => ToolStore::new(paths.0.config(), paths.0.home()),
            None => ToolStore::current(),
        };
        commands.spawn((
            Operation::bundle("mcp.snapshot").unwrap().running(),
            OperationTarget::new(target),
            OperationRetained,
            SnapshotContext {
                generation: request.generation,
                sequence: version.next,
                owner,
                source_tick: credentials.last_changed(),
                result: request.result.clone(),
            },
            auth::Credentials((*credentials).clone()),
            store,
            ReadRequest { retain_lease: true },
        ));
    }
}

fn cancel(
    snapshots: Query<
        (
            Entity,
            &SnapshotContext,
            Option<&OperationTarget>,
            &OperationState,
        ),
        Without<Refused>,
    >,
    states: Query<&McpPageState>,
    credentials: Query<(Entity, Ref<McpCredentials>)>,
    version: Res<McpObservationVersion>,
    mut commands: Commands,
) {
    for (entity, snapshot, target, state) in &snapshots {
        let current = credentials.single().is_ok_and(|(owner, source)| {
            owner == snapshot.owner && source.last_changed() == snapshot.source_tick
        });
        let page = target.and_then(|target| states.get(target.entity()).ok());
        let reason = if !current {
            "MCP source Profile was replaced"
        } else if page.is_none_or(|page| {
            page.generation != snapshot.generation || page.owner != Some(snapshot.owner)
        }) {
            "MCP snapshot page was retired"
        } else if !matches!(state, OperationState::Pending | OperationState::Running) {
            "MCP snapshot was canceled"
        } else if snapshot.sequence < version.current {
            "MCP snapshot was superseded"
        } else {
            continue;
        };
        commands
            .entity(entity)
            .insert((Refused(reason.into()), OperationState::Canceled));
    }
}

fn manifest(
    snapshots: Query<(Entity, &ReadResult, Has<Refused>), With<SnapshotContext>>,
    mut commands: Commands,
) {
    for (entity, result, refused) in &snapshots {
        commands
            .entity(entity)
            .remove::<ReadResult>()
            .insert(SnapshotManifest(result.0.clone()));
        if refused || result.0.is_err() {
            commands.entity(entity).insert(Ready);
        }
    }
}

fn lease(
    snapshots: Query<
        (Entity, &auth::Credentials),
        (
            With<SnapshotManifest>,
            Without<CredentialLease>,
            Without<CredentialLeaseTask>,
            Without<Ready>,
            Without<Refused>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, credentials) in &snapshots {
        let access = credentials.0.access.clone();
        commands
            .entity(entity)
            .insert(CredentialLeaseTask(IoTaskPool::get().spawn(async move {
                access
                    .transaction()
                    .await
                    .map(|lease| CredentialLease { _lease: lease })
            })));
    }
}

fn schedule(
    snapshots: Query<
        (Entity, &SnapshotManifest, &auth::Credentials),
        (
            With<CredentialLease>,
            Without<Reading>,
            Without<Refused>,
            Without<Ready>,
        ),
    >,
    catalog: Query<(Entity, Ref<McpCatalogServer>)>,
    mut commands: Commands,
) {
    for (entity, manifest, credentials) in &snapshots {
        if manifest.0.is_err() {
            commands.entity(entity).insert(Ready);
            continue;
        }
        for (server_entity, server) in &catalog {
            commands.spawn((
                Operation::bundle("mcp.credential.read").unwrap().running(),
                OperationRetained,
                EntityTarget::<SnapshotContext>::new(entity),
                EntityTarget::<McpCatalogServer>::new(server_entity),
                CredentialReadRequest {
                    id: server.id.clone(),
                    source_tick: server.last_changed(),
                    storage: credentials.0.storage.clone(),
                },
            ));
        }
        commands.entity(entity).insert(Reading);
    }
}

fn poll_lease(
    mut tasks: Query<(Entity, &mut CredentialLeaseTask)>,
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
        commands.entity(entity).remove::<CredentialLeaseTask>();
        match result {
            Ok(lease) => {
                commands.entity(entity).insert(lease);
            }
            Err(error) => {
                commands.entity(entity).insert((Refused(error), Ready));
            }
        }
    }
}

fn poll_credentials(
    requests: Query<
        (
            Entity,
            &EntityTarget<SnapshotContext>,
            &CredentialReadRequest,
        ),
        Without<CredentialReadTask>,
    >,
    snapshots: Query<(), (With<SnapshotContext>, Without<Refused>)>,
    catalog: Query<Ref<McpCatalogServer>>,
    servers: Query<&EntityTarget<McpCatalogServer>>,
    mut tasks: Query<(Entity, &mut CredentialReadTask)>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, parent, request) in &requests {
        let current = servers.get(entity).is_ok_and(|target| {
            catalog.get(target.entity()).is_ok_and(|server| {
                server.id == request.id && server.last_changed() == request.source_tick
            })
        });
        if !snapshots.contains(parent.entity()) || !current {
            commands
                .entity(entity)
                .remove::<CredentialReadRequest>()
                .insert(CredentialReadResult(Err(
                    "MCP credential read canceled".into()
                )));
            continue;
        }
        let request = CredentialReadRequest {
            id: request.id.clone(),
            source_tick: request.source_tick,
            storage: request.storage.clone(),
        };
        commands.entity(entity).insert(CredentialReadTask(
            IoTaskPool::get().spawn(async move { request.load() }),
        ));
    }
    let waker = wake
        .as_deref()
        .map(|wake| &wake.0)
        .unwrap_or(std::task::Waker::noop());
    let mut context = Context::from_waker(waker);
    for (entity, mut task) in &mut tasks {
        let Poll::Ready(result) = Pin::new(&mut task.0).poll(&mut context) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<CredentialReadTask>()
            .insert(CredentialReadResult(result));
    }
}

fn drain(
    reads: Query<(
        Entity,
        &EntityTarget<SnapshotContext>,
        &EntityTarget<McpCatalogServer>,
        &CredentialReadResult,
        Option<&CredentialReadRequest>,
    )>,
    snapshots: Query<(&SnapshotManifest, Has<Refused>)>,
    catalog: Query<Ref<McpCatalogServer>>,
    mut commands: Commands,
) {
    for (entity, parent, server_entity, result, request) in &reads {
        commands.entity(entity).remove::<(
            CredentialReadRequest,
            CredentialReadResult,
            OperationRetained,
        )>();
        let Ok((manifest, refused)) = snapshots.get(parent.entity()) else {
            commands.entity(entity).despawn();
            continue;
        };
        let server = catalog.get(server_entity.entity()).ok();
        let current = request.is_some_and(|request| {
            server.as_ref().is_some_and(|server| {
                request.source_tick == server.last_changed() && request.id == server.id
            })
        });
        let outcome = if refused || !current {
            Err("MCP credential observation was retired".to_owned())
        } else {
            result.0.as_ref().map(|_| ()).map_err(Clone::clone)
        };
        commands.trigger(OperationCompleted {
            entity,
            result: outcome,
        });
        if !current && !refused {
            commands
                .entity(parent.entity())
                .insert(Refused("MCP catalog server was replaced".into()));
        }
        let Some(server) = server else {
            commands.entity(entity).insert(SnapshotServerFacts {
                source_tick: Tick::new(0),
                facts: McpServerFacts::default(),
            });
            continue;
        };
        let occupied = manifest
            .0
            .as_ref()
            .is_ok_and(|manifest| manifest.mcp.servers.contains_key(&server.id));
        let owned = manifest.0.as_ref().is_ok_and(|manifest| {
            manifest
                .mcp
                .servers
                .get(&server.id)
                .is_some_and(|configured| server.owns(configured))
        });
        let authenticated = current
            && !refused
            && owned
            && result.0.as_ref().is_ok_and(|secret| {
                secret.as_ref().is_some_and(|secret| {
                    server.authorizes(&secret.0.resource)
                        && (!secret.0.expires_soon()
                            || secret
                                .0
                                .refresh_token
                                .as_ref()
                                .is_some_and(|token| !token.is_empty()))
                })
            });
        let account = if current && !refused {
            result
                .0
                .as_ref()
                .ok()
                .and_then(|secret| secret.as_ref())
                .and_then(|secret| secret.0.account.clone())
        } else {
            None
        };
        commands.entity(entity).insert(SnapshotServerFacts {
            source_tick: server.last_changed(),
            facts: McpServerFacts {
                occupied,
                owned,
                authenticated,
                readable: current && !refused && result.0.is_ok(),
                account,
            },
        });
    }
}

fn materialize(
    snapshots: Query<(Entity, &SnapshotContext, &SnapshotManifest, Has<Refused>), With<Reading>>,
    reads: Query<(
        &EntityTarget<SnapshotContext>,
        &EntityTarget<McpCatalogServer>,
        Option<&SnapshotServerFacts>,
    )>,
    catalog: Query<(Entity, Ref<McpCatalogServer>)>,
    extras: Query<Entity, With<McpConfiguredServer>>,
    mut version: ResMut<McpObservationVersion>,
    mut commands: Commands,
) {
    let mut latest = version.current;
    for (entity, snapshot, manifest, refused) in &snapshots {
        if refused || manifest.0.is_err() {
            continue;
        }
        if reads
            .iter()
            .any(|(parent, _, facts)| parent.entity() == entity && facts.is_none())
        {
            continue;
        }
        latest = latest.max(snapshot.sequence);
    }
    for (entity, snapshot, manifest, refused) in &snapshots {
        if reads
            .iter()
            .any(|(parent, _, facts)| parent.entity() == entity && facts.is_none())
        {
            continue;
        }
        commands.entity(entity).remove::<Reading>().insert(Ready);
        if refused || snapshot.sequence < latest {
            continue;
        }
        let Ok(manifest) = &manifest.0 else {
            continue;
        };
        let mut complete = true;
        for (server, source) in &catalog {
            if !reads.iter().any(|(parent, target, facts)| {
                parent.entity() == entity
                    && target.entity() == server
                    && facts.is_some_and(|facts| facts.source_tick == source.last_changed())
            }) {
                complete = false;
                break;
            }
        }
        if !complete {
            commands
                .entity(entity)
                .insert(Refused("MCP catalog changed while reading".into()));
            continue;
        }
        for (parent, server, facts) in &reads {
            if parent.entity() == entity
                && let Some(facts) = facts
            {
                commands.entity(server.entity()).insert(facts.facts.clone());
            }
        }
        for old in &extras {
            commands.entity(old).despawn();
        }
        let mut ids = BTreeSet::new();
        for (_, server) in &catalog {
            ids.insert(server.id.clone());
        }
        for (id, server) in &manifest.mcp.servers {
            if ids.contains(id.as_str()) || id == "vmux" {
                continue;
            }
            let description = server
                .url
                .clone()
                .or_else(|| server.command.clone())
                .unwrap_or_else(|| format!("{:?}", server.transport).to_ascii_lowercase());
            commands.spawn((
                Name::new(id.clone()),
                McpConfiguredServer {
                    id: id.clone(),
                    description,
                },
            ));
        }
        version.current = snapshot.sequence;
        version.owner = Some(snapshot.owner);
        version.source_tick = Some(snapshot.source_tick);
        version.valid = true;
    }
}

fn finish(
    snapshots: Query<
        (
            Entity,
            &SnapshotContext,
            Option<&SnapshotManifest>,
            Option<&Refused>,
            Option<&OperationTarget>,
        ),
        (
            Without<ReadRequest>,
            Without<CredentialLeaseTask>,
            Or<(With<Ready>, With<Refused>)>,
        ),
    >,
    reads: Query<(
        Entity,
        &EntityTarget<SnapshotContext>,
        Has<CredentialReadTask>,
        Has<CredentialReadRequest>,
    )>,
    mut states: Query<&mut McpPageState>,
    mut version: ResMut<McpObservationVersion>,
    mut commands: Commands,
) {
    for (entity, snapshot, manifest, refused, target) in &snapshots {
        if reads
            .iter()
            .any(|(_, parent, task, pending)| parent.entity() == entity && (task || pending))
        {
            continue;
        }
        let result = match (refused, manifest) {
            (Some(reason), _) => Err(reason.0.clone()),
            (_, Some(manifest)) => manifest.0.as_ref().map(|_| ()).map_err(Clone::clone),
            _ => Err("MCP snapshot has no manifest".into()),
        };
        if refused.is_none() && result.is_err() && snapshot.sequence >= version.current {
            version.current = snapshot.sequence;
            version.owner = Some(snapshot.owner);
            version.source_tick = Some(snapshot.source_tick);
            version.valid = false;
        }
        for mut state in &mut states {
            if state.owner == Some(snapshot.owner) {
                state.set_changed();
            }
        }
        if let Some(target) = target
            && let Ok(mut state) = states.get_mut(target.entity())
            && state.generation == snapshot.generation
        {
            state.loading = false;
            state.pending = false;
            state.loaded = result.is_ok() && version.valid && version.owner == Some(snapshot.owner);
            let completion = match &result {
                Ok(()) => snapshot.result.clone(),
                Err(error) => Some(McpCompletion {
                    id: String::new(),
                    disconnect: false,
                    success: false,
                    message: error.clone(),
                }),
            };
            if let Some(completion) = completion {
                commands.entity(target.entity()).insert(completion);
            }
        }
        for (read, parent, _, _) in &reads {
            if parent.entity() == entity {
                commands.entity(read).despawn();
            }
        }
        commands.entity(entity).remove::<(
            SnapshotContext,
            SnapshotManifest,
            CredentialLease,
            StoreLease,
            ToolStore,
            auth::Credentials,
            OperationRetained,
            Ready,
            Reading,
            Refused,
        )>();
        commands.trigger(OperationCompleted { entity, result });
    }
}

fn wake(
    pending: Query<
        Entity,
        Or<(
            Added<SnapshotContext>,
            Added<CredentialLeaseTask>,
            Added<CredentialLease>,
            Added<CredentialReadTask>,
            Added<CredentialReadResult>,
            Added<ReadResult>,
        )>,
    >,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    if !pending.is_empty()
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::{TaskPoolBuilder, futures_lite::future};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use vmux_profile::mcp_credentials::McpCredentialAccess;

    struct Fixture {
        app: App,
        store: ToolStore,
        owner: Entity,
        page: Entity,
        server: Entity,
        access: McpCredentialAccess,
        directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(
                directory.path().join("config"),
                directory.path().join("home"),
            );
            let profile = vmux_profile::Profile::named("snapshot-contract");
            let credentials = McpCredentials {
                access: McpCredentialAccess::default(),
                storage: McpCredentialStorage::at(
                    profile.clone(),
                    vmux_profile::ProfilePaths::at(profile, directory.path().to_path_buf()),
                ),
            };
            let access = credentials.access.clone();
            let mut app = App::new();
            app.add_plugins(super::super::McpConnectionPlugin);
            let owner = app.world_mut().spawn(credentials).id();
            let page = app
                .world_mut()
                .spawn(McpPageState {
                    generation: 1,
                    owner: Some(owner),
                    loading: true,
                    ..Default::default()
                })
                .id();
            let server = app
                .world_mut()
                .spawn((
                    Name::new("Linear"),
                    McpCatalogServer {
                        setup_url: None,
                        client_id: None,
                        client_id_env: None,
                        account_endpoint: None,
                        id: "linear".into(),
                        url: "https://mcp.linear.app/mcp".into(),
                        scopes: Vec::new(),
                    },
                    super::super::McpServerClaim::default(),
                ))
                .id();
            app.update();
            Self {
                app,
                store,
                owner,
                page,
                server,
                access,
                directory,
            }
        }

        fn snapshot(&mut self) -> Entity {
            let credentials = self
                .app
                .world()
                .get::<McpCredentials>(self.owner)
                .unwrap()
                .clone();
            let tick = self
                .app
                .world()
                .entity(self.owner)
                .get_ref::<McpCredentials>()
                .unwrap()
                .last_changed();
            self.app
                .world_mut()
                .spawn((
                    Operation::bundle("mcp.snapshot").unwrap().running(),
                    OperationRetained,
                    OperationTarget::new(self.page),
                    SnapshotContext {
                        generation: 1,
                        sequence: 1,
                        owner: self.owner,
                        source_tick: tick,
                        result: None,
                    },
                    auth::Credentials(credentials),
                    self.store.clone(),
                ))
                .id()
        }

        fn blocked_read(&mut self, resource: &str) -> (Entity, Entity, Arc<async_lock::Semaphore>) {
            let operation = self.snapshot();
            let server = self
                .app
                .world()
                .get::<McpCatalogServer>(self.server)
                .unwrap();
            let mut manifest = ToolsManifest::default();
            manifest
                .mcp
                .servers
                .insert(server.id.clone(), server.manifest());
            manifest.write_to(&self.store.manifest_path()).unwrap();
            let store_lease = self.store.lease().unwrap();
            let lease = future::block_on(self.access.transaction()).unwrap();
            self.app.world_mut().entity_mut(operation).insert((
                SnapshotManifest(Ok(manifest)),
                Reading,
                CredentialLease { _lease: lease },
                StoreLease {
                    _lease: store_lease,
                },
            ));
            let gate = Arc::new(async_lock::Semaphore::new(0));
            let held = Arc::clone(&gate);
            let resource = resource.to_owned();
            let request = self.read_request();
            let read = self
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("mcp.credential.read").unwrap().running(),
                    OperationRetained,
                    EntityTarget::<SnapshotContext>::new(operation),
                    EntityTarget::<McpCatalogServer>::new(self.server),
                    request,
                    CredentialReadTask(IoTaskPool::get().spawn(async move {
                        let _guard = held.acquire_arc().await;
                        Ok(Some(SecretHandle(McpOauthCredentials {
                            account: None,
                            resource,
                            client_secret: Some("test-client-secret".into()),
                            access_token: "test-access-token".into(),
                            refresh_token: Some("test-refresh-token".into()),
                            ..Default::default()
                        })))
                    })),
                ))
                .id();
            (operation, read, gate)
        }

        fn read_request(&self) -> CredentialReadRequest {
            let entity = self.app.world().entity(self.server);
            let server = entity.get_ref::<McpCatalogServer>().unwrap();
            CredentialReadRequest {
                id: server.id.clone(),
                source_tick: server.last_changed(),
                storage: self
                    .app
                    .world()
                    .get::<McpCredentials>(self.owner)
                    .unwrap()
                    .storage
                    .clone(),
            }
        }

        fn finish(&mut self, operation: Entity) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.app.world().get::<SnapshotContext>(operation).is_some() {
                self.app.update();
                assert!(Instant::now() < deadline, "snapshot did not drain");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    #[test]
    fn complete_snapshot_materializes_each_catalog_slot_and_keeps_foreign_configuration() {
        let mut fixture = Fixture::new();
        let first = fixture
            .app
            .world()
            .get::<McpCatalogServer>(fixture.server)
            .unwrap()
            .manifest();
        let foreign = fixture
            .app
            .world_mut()
            .spawn((
                Name::new("Other"),
                McpCatalogServer {
                    setup_url: None,
                    client_id: None,
                    client_id_env: None,
                    account_endpoint: None,
                    id: "other".into(),
                    url: "https://other.example/mcp".into(),
                    scopes: Vec::new(),
                },
                super::super::McpServerClaim::default(),
            ))
            .id();
        let mut manifest = ToolsManifest::default();
        manifest.mcp.servers.insert("linear".into(), first.clone());
        manifest.mcp.servers.insert("other".into(), first.clone());
        manifest.mcp.servers.insert("custom".into(), first);
        manifest.write_to(&fixture.store.manifest_path()).unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let operation = fixture.snapshot();
        fixture
            .app
            .world_mut()
            .entity_mut(operation)
            .insert(ReadRequest { retain_lease: true });
        fixture.finish(operation);
        let first = fixture
            .app
            .world()
            .get::<McpServerFacts>(fixture.server)
            .unwrap();
        assert!(first.owned && first.occupied && first.readable && !first.authenticated);
        let second = fixture.app.world().get::<McpServerFacts>(foreign).unwrap();
        assert!(!second.owned && second.occupied && second.readable && !second.authenticated);
        let extras = fixture
            .app
            .world_mut()
            .query::<(&Name, &McpConfiguredServer)>()
            .iter(fixture.app.world())
            .map(|(name, server)| (name.as_str().to_owned(), server.id.clone()))
            .collect::<Vec<_>>();
        assert_eq!(extras, [("custom".to_owned(), "custom".to_owned())]);
        assert!(
            fixture
                .app
                .world()
                .get::<McpPageState>(fixture.page)
                .unwrap()
                .loaded
        );
        assert!(
            fixture
                .app
                .world()
                .resource::<McpObservationVersion>()
                .valid
        );
        assert!(fixture.store.lease().is_some());
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
    }

    #[test]
    fn canceled_manifest_admission_waits_for_storage_drain_and_never_reads_credentials() {
        let mut fixture = Fixture::new();
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "preserved", true);
        manifest.write_to(&fixture.store.manifest_path()).unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let blocker = fixture.store.lease().unwrap();
        let operation = fixture.snapshot();
        fixture
            .app
            .world_mut()
            .entity_mut(operation)
            .insert(ReadRequest { retain_lease: true });
        fixture.app.update();
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .entity_mut(operation)
            .insert(OperationState::Canceled);
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<OperationRetained>(operation)
                .is_some()
        );
        assert!(fixture.app.world().get::<ReadRequest>(operation).is_some());
        drop(blocker);
        fixture.finish(operation);
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(fixture.store.lease().is_some());
        assert_eq!(fixture.access.revision(), 0);
        assert!(
            !fixture
                .app
                .world()
                .resource::<McpObservationVersion>()
                .valid
        );
    }

    #[test]
    fn native_credential_facts_authorize_only_the_owned_https_resource() {
        for (resource, authorized) in [
            ("https://mcp.linear.app/mcp", true),
            ("https://MCP.LINEAR.APP:443/mcp", true),
            ("https://other.example/mcp", false),
            ("http://mcp.linear.app/mcp", false),
        ] {
            let mut fixture = Fixture::new();
            let revision = fixture.access.stable_revision().unwrap();
            let (operation, read, gate) = fixture.blocked_read(resource);
            fixture.app.update();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<McpServerFacts>(fixture.server)
                    .is_none()
            );
            assert_eq!(
                fixture
                    .access
                    .with_revision(revision, || "prepared")
                    .unwrap(),
                None
            );
            gate.add_permits(1);
            fixture.finish(operation);
            let facts = fixture
                .app
                .world()
                .get::<McpServerFacts>(fixture.server)
                .unwrap();
            assert!(facts.owned && facts.occupied && facts.readable);
            assert_eq!(facts.authenticated, authorized);
            assert!(fixture.app.world().get_entity(read).is_err());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<OperationRetained>(operation)
                    .is_none()
            );
            assert!(fixture.store.lease().is_some());
            assert_eq!(
                fixture.access.with_revision(revision, || "stale").unwrap(),
                None
            );
            let revision = fixture.access.stable_revision().unwrap();
            assert_eq!(
                fixture
                    .access
                    .with_revision(revision, || "reprepared")
                    .unwrap(),
                Some("reprepared")
            );
        }
    }

    #[test]
    fn retired_sources_and_generations_keep_jobs_and_leases_until_late_delivery_drains() {
        for retirement in [
            "owner",
            "same-owner",
            "page",
            "cancel",
            "sequence",
            "catalog",
            "catalog-add",
            "catalog-remove",
            "page-remove",
        ] {
            let mut fixture = Fixture::new();
            let (operation, read, gate) = fixture.blocked_read("https://mcp.linear.app/mcp");
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.server)
                .insert(McpServerFacts {
                    occupied: true,
                    owned: false,
                    authenticated: false,
                    readable: false,
                    account: None,
                });
            match retirement {
                "owner" => {
                    fixture.app.world_mut().despawn(fixture.owner);
                }
                "same-owner" => {
                    let credentials = McpCredentials {
                        access: McpCredentialAccess::default(),
                        storage: McpCredentialStorage::at(
                            vmux_profile::Profile::named("replacement"),
                            vmux_profile::ProfilePaths::at(
                                vmux_profile::Profile::named("replacement"),
                                fixture.directory.path().to_path_buf(),
                            ),
                        ),
                    };
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.owner)
                        .insert(credentials);
                }
                "page" => {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<McpPageState>(fixture.page)
                        .unwrap()
                        .generation = 2;
                }
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(operation)
                        .insert(OperationState::Canceled);
                }
                "sequence" => {
                    fixture
                        .app
                        .world_mut()
                        .resource_mut::<McpObservationVersion>()
                        .current = 2;
                }
                "catalog-add" => {
                    fixture.app.world_mut().spawn(McpCatalogServer {
                        setup_url: None,
                        client_id: None,
                        client_id_env: None,
                        account_endpoint: None,
                        id: "additional".into(),
                        url: "https://additional.example/mcp".into(),
                        scopes: Vec::new(),
                    });
                }
                "catalog-remove" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.server)
                        .remove::<McpCatalogServer>();
                }
                "page-remove" => {
                    fixture.app.world_mut().despawn(fixture.page);
                }
                "catalog" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.server)
                        .insert(McpCatalogServer {
                            setup_url: None,
                            client_id: None,
                            client_id_env: None,
                            account_endpoint: None,
                            id: "replacement".into(),
                            url: "https://other.example/mcp".into(),
                            scopes: Vec::new(),
                        });
                }
                _ => unreachable!(),
            }
            fixture.app.update();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<CredentialReadTask>(read)
                    .is_some(),
                "{retirement}"
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<OperationRetained>(operation)
                    .is_some(),
                "{retirement}"
            );
            assert!(
                fixture.app.world().get::<StoreLease>(operation).is_some(),
                "{retirement}"
            );
            assert!(
                fixture
                    .access
                    .with_revision(fixture.access.revision(), || ())
                    .unwrap()
                    .is_none()
            );
            gate.add_permits(1);
            fixture.finish(operation);
            let facts = fixture
                .app
                .world()
                .get::<McpServerFacts>(fixture.server)
                .unwrap();
            assert!(
                !facts.owned && !facts.authenticated && !facts.readable,
                "{retirement}"
            );
            assert!(fixture.store.lease().is_some());
            assert!(fixture.access.stable_revision().unwrap().is_multiple_of(2));
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before
            );
            assert!(
                !fixture
                    .app
                    .world()
                    .resource::<McpObservationVersion>()
                    .valid
            );
            if retirement == "page" {
                let state = fixture
                    .app
                    .world()
                    .get::<McpPageState>(fixture.page)
                    .unwrap();
                assert_eq!(state.generation, 2);
                assert!(state.loading);
            }
        }
    }

    #[test]
    fn canceled_queued_credentials_never_start_native_storage() {
        let mut fixture = Fixture::new();
        let operation = fixture.snapshot();
        let lease = future::block_on(fixture.access.transaction()).unwrap();
        fixture.app.world_mut().entity_mut(operation).insert((
            SnapshotManifest(Ok(ToolsManifest::default())),
            Reading,
            OperationState::Canceled,
            CredentialLease { _lease: lease },
            StoreLease {
                _lease: fixture.store.lease().unwrap(),
            },
        ));
        let request = fixture.read_request();
        let read = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.credential.read").unwrap().running(),
                OperationRetained,
                EntityTarget::<SnapshotContext>::new(operation),
                EntityTarget::<McpCatalogServer>::new(fixture.server),
                request,
            ))
            .id();
        fixture.finish(operation);
        assert!(fixture.app.world().get_entity(read).is_err());
        assert!(!fixture.store.manifest_path().exists());
        assert!(fixture.store.lease().is_some());
        assert!(
            !fixture
                .app
                .world()
                .resource::<McpObservationVersion>()
                .valid
        );
    }

    #[test]
    fn snapshot_from_an_old_profile_cannot_replace_current_global_observations() {
        let mut fixture = Fixture::new();
        let (operation, _, gate) = fixture.blocked_read("https://mcp.linear.app/mcp");
        let old = fixture
            .app
            .world()
            .get::<McpCredentials>(fixture.owner)
            .unwrap()
            .clone();
        fixture.app.world_mut().despawn(fixture.owner);
        let current = fixture.app.world_mut().spawn(old).id();
        let tick = fixture
            .app
            .world()
            .entity(current)
            .get_ref::<McpCredentials>()
            .unwrap()
            .last_changed();
        {
            let mut version = fixture
                .app
                .world_mut()
                .resource_mut::<McpObservationVersion>();
            version.current = 2;
            version.owner = Some(current);
            version.source_tick = Some(tick);
            version.valid = true;
        }
        gate.add_permits(1);
        fixture.finish(operation);
        let version = fixture.app.world().resource::<McpObservationVersion>();
        assert_eq!(version.current, 2);
        assert_eq!(version.owner, Some(current));
        assert!(version.valid);
        assert!(
            !fixture
                .app
                .world()
                .get::<McpPageState>(fixture.page)
                .unwrap()
                .loading
        );
    }

    #[test]
    fn failed_manifest_read_drains_without_credential_io_or_authorization() {
        let mut fixture = Fixture::new();
        std::fs::create_dir_all(fixture.store.root()).unwrap();
        std::fs::write(fixture.store.manifest_path(), "version = 999\n").unwrap();
        let operation = fixture.snapshot();
        fixture
            .app
            .world_mut()
            .entity_mut(operation)
            .insert(ReadRequest { retain_lease: true });
        fixture.finish(operation);
        assert!(
            !fixture
                .app
                .world()
                .resource::<McpObservationVersion>()
                .valid
        );
        assert!(
            fixture
                .app
                .world()
                .get::<McpServerFacts>(fixture.server)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<CredentialLease>(operation)
                .is_none()
        );
        assert!(fixture.store.lease().is_some());
        assert_eq!(
            std::fs::read_to_string(fixture.store.manifest_path()).unwrap(),
            "version = 999\n"
        );
        assert_eq!(fixture.access.revision(), 0);
    }

    #[test]
    fn profile_replacement_while_waiting_for_a_credential_lease_drains_the_accepted_acquisition() {
        let mut fixture = Fixture::new();
        let blocker = future::block_on(fixture.access.transaction()).unwrap();
        let operation = fixture.snapshot();
        fixture.app.world_mut().entity_mut(operation).insert((
            SnapshotManifest(Ok(ToolsManifest::default())),
            StoreLease {
                _lease: fixture.store.lease().unwrap(),
            },
        ));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<CredentialLeaseTask>(operation)
                .is_some()
        );
        fixture.app.world_mut().despawn(fixture.owner);
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<CredentialLeaseTask>(operation)
                .is_some()
        );
        assert!(fixture.store.lease().is_none());
        drop(blocker);
        fixture.finish(operation);
        assert!(fixture.store.lease().is_some());
        assert!(fixture.access.stable_revision().unwrap().is_multiple_of(2));
        assert!(
            !fixture
                .app
                .world()
                .resource::<McpObservationVersion>()
                .valid
        );
    }
}
