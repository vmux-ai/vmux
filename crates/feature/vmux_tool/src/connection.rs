use std::collections::BTreeSet;

use crate::{McpServerManifest, McpTransport, ToolStore, ToolsManifest};
use bevy::app::{App, Plugin};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_api::mcp::{
    McpServerEntry, McpServerOperation, McpServerPending, McpServerRequest, McpServerResult,
    McpServerStatus, McpServersRequest, McpServersUiState,
};
use vmux_ecs::profile::McpCredentials;
use vmux_ecs::profile::mcp_credentials::McpOauthCredentials;
use vmux_ecs::{EntityTarget, PageOpenRequest, PageOpenTarget};
use vmux_ecs::{
    Operation, OperationCompleted, OperationPlugin, OperationRetained, OperationTarget,
    UiStatePlugin, UiStateWrite,
};
use vmux_feature::manifest::FeatureManifest;

mod auth;
mod remove;
mod transport;

pub struct McpConnectionPlugin;

impl Plugin for McpConnectionPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        if !app.is_plugin_added::<vmux_ecs::wake::WakePlugin>() {
            app.add_plugins(vmux_ecs::wake::WakePlugin);
        }
        app.add_plugins((auth::AuthPlugin, remove::RemovePlugin))
            .init_resource::<McpObservationVersion>()
            .add_plugins((
                UiEventPlugin::<(McpServersRequest, McpServerRequest)>::default(),
                UiStatePlugin::<McpServersUiState>::default(),
            ))
            .add_systems(Startup, spawn_mcp_catalog)
            .add_message::<PageOpenRequest>()
            .add_observer(request_mcp_connections)
            .add_observer(begin_mcp_snapshot)
            .add_observer(request_mcp_server)
            .add_systems(
                Update,
                (
                    publish_mcp_authorization,
                    retire_mcp_operations,
                    start_mcp_snapshots,
                    drain_mcp_snapshots,
                    publish_mcp_connections,
                )
                    .chain()
                    .after(auth::AuthSet)
                    .after(remove::RemoveSet),
            );
    }
}

fn request_mcp_connections(
    trigger: On<UiInput<McpServersRequest>>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    if !browsers
        .as_deref()
        .is_some_and(|browsers| browsers.can_emit_to(&target))
    {
        return;
    }
    commands.trigger(McpSnapshotRequest { target });
}

fn begin_mcp_snapshot(
    trigger: On<McpSnapshotRequest>,
    mut states: Query<&mut McpPageState>,
    mut commands: Commands,
) {
    let target = trigger.event().target;
    let generation = match states.get_mut(target) {
        Ok(mut state) => {
            if state.loading || state.pending {
                return;
            }
            state.generation = state.generation.wrapping_add(1).max(1);
            state.loading = true;
            commands.entity(target).remove::<McpCompletion>();
            state.generation
        }
        Err(_) => {
            let state = McpPageState {
                generation: 1,
                loading: true,
                ..Default::default()
            };
            let generation = state.generation;
            commands.entity(target).insert(state);
            generation
        }
    };
    commands.entity(target).insert(PendingMcpSnapshot {
        generation,
        result: None,
    });
}

fn request_mcp_server(
    trigger: On<UiInput<McpServerRequest>>,
    mut states: Query<&mut McpPageState>,
    mut servers: Query<(
        Entity,
        &McpCatalogServer,
        &McpServerFacts,
        &mut McpServerClaim,
    )>,
    version: Res<McpObservationVersion>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let id = &trigger.event().payload.id;
    let Ok(mut state) = states.get_mut(target) else {
        return;
    };
    if state.loading
        || state.pending
        || !version.valid
        || version.owner.is_none()
        || state.owner != version.owner
    {
        return;
    }
    let mut selected = None;
    for (entity, server, facts, claim) in &mut servers {
        if &server.id == id && claim.operation.is_none() {
            selected = Some((entity, facts, claim));
            break;
        }
    }
    let Some((_server_entity, server, mut claim)) = selected else {
        return;
    };
    if !server.readable || server.occupied && !server.owned {
        return;
    }
    let disconnect = server.owned && server.authenticated;
    state.generation = state.generation.wrapping_add(1).max(1);
    state.pending = true;
    let operation = commands
        .spawn((
            Operation::bundle(if disconnect {
                "mcp.disconnect"
            } else {
                "mcp.connect"
            })
            .unwrap()
            .running(),
            OperationRetained,
            OperationTarget::new(target),
            EntityTarget::<McpCatalogServer>::new(_server_entity),
            McpConnectionOperation,
            PendingMcpOperation {
                generation: state.generation,
                owner: version.owner.unwrap(),
                id: id.clone(),
                disconnect,
            },
        ))
        .id();
    commands
        .entity(target)
        .remove::<McpCompletion>()
        .insert(EntityTarget::<McpConnectionOperation>::new(operation));
    claim.operation = Some(operation);
}

fn publish_mcp_authorization(
    urls: Query<(Entity, &McpAuthorizationUrl, Option<&OperationTarget>)>,
    states: Query<(&McpPageState, &EntityTarget<McpConnectionOperation>)>,
    browsers: Option<NonSend<Browsers>>,
    mut pages: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    for (entity, url, target) in &urls {
        commands.entity(entity).remove::<McpAuthorizationUrl>();
        let Some(target) = target else {
            continue;
        };
        let page = target.entity();
        if !states.get(page).is_ok_and(|(state, binding)| {
            state.generation == url.generation && binding.entity() == entity
        }) {
            continue;
        }
        if !browsers
            .as_deref()
            .is_some_and(|browsers| browsers.can_emit_to(&page))
        {
            continue;
        }
        pages.write(PageOpenRequest {
            target: PageOpenTarget::NewStack,
            url: url.url.clone(),
            request_id: None,
        });
    }
}

fn retire_mcp_operations(
    done: Query<
        (
            Entity,
            &McpCompletion,
            Option<&OperationTarget>,
            &EntityTarget<McpCatalogServer>,
        ),
        (
            With<McpConnectionOperation>,
            Without<auth::Active>,
            Without<remove::Active>,
            Without<PendingMcpOperation>,
        ),
    >,
    mut states: Query<(&mut McpPageState, &EntityTarget<McpConnectionOperation>)>,
    mut servers: Query<&mut McpServerClaim, With<McpCatalogServer>>,
    mut commands: Commands,
) {
    for (entity, result, target, server) in &done {
        if let Ok(mut claim) = servers.get_mut(server.entity())
            && claim.operation == Some(entity)
        {
            claim.operation = None;
        }
        if let Some(target) = target
            && let Ok((mut state, binding)) = states.get_mut(target.entity())
            && binding.entity() == entity
        {
            commands
                .entity(target.entity())
                .remove::<EntityTarget<McpConnectionOperation>>();
            commands.entity(target.entity()).insert(PendingMcpSnapshot {
                generation: state.generation,
                result: Some(result.clone()),
            });
            state.loading = true;
        }
        commands
            .entity(entity)
            .remove::<(McpConnectionOperation, OperationRetained, McpCompletion)>();
    }
}

fn start_mcp_snapshots(
    requests: Query<(Entity, &PendingMcpSnapshot), Without<McpSnapshotTask>>,
    catalog: McpCatalog,
    credentials: Query<(Entity, &McpCredentials)>,
    mut version: ResMut<McpObservationVersion>,
    mut states: Query<&mut McpPageState>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok((owner, credentials)) = credentials.single() else {
        version.valid = false;
        version.owner = None;
        for (target, request) in &requests {
            if let Ok(mut state) = states.get_mut(target) {
                state.loading = false;
                state.pending = false;
                state.loaded = false;
                state.owner = None;
            }
            commands.entity(target).remove::<PendingMcpSnapshot>();
            commands
                .entity(target)
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
        let Ok(mut state) = states.get_mut(target) else {
            continue;
        };
        state.owner = Some(owner);
        version.next = version.next.wrapping_add(1).max(1);
        let sequence = version.next;
        let generation = request.generation;
        let result = request.result.clone();
        let mut ids = Vec::new();
        for (_, server) in &catalog.servers {
            ids.push(server.id.clone());
        }
        let credentials = (*credentials).clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        commands.entity(target).remove::<PendingMcpSnapshot>();
        commands.spawn((
            Operation::bundle("mcp.snapshot").unwrap().running(),
            OperationTarget::new(target),
            OperationRetained,
            McpSnapshotTask {
                generation,
                sequence,
                owner,
                result,
                task: IoTaskPool::get().spawn(async move {
                    let store = ToolStore::current();
                    let _lease = store.lease_async().await;
                    let manifest = store.load();
                    let mut records = Vec::new();
                    for id in ids {
                        records.push((id.clone(), credentials.storage.load(&id)));
                    }
                    if let Some(wake) = wake {
                        let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                    }
                    (manifest, records)
                }),
            },
        ));
    }
}

fn drain_mcp_snapshots(
    mut tasks: Query<(Entity, &mut McpSnapshotTask, Option<&OperationTarget>)>,
    mut states: Query<&mut McpPageState>,
    catalog: Query<(Entity, &Name, &McpCatalogServer)>,
    extras: Query<Entity, With<McpConfiguredServer>>,
    credentials: Query<Entity, With<McpCredentials>>,
    mut version: ResMut<McpObservationVersion>,
    mut commands: Commands,
) {
    for (entity, mut task, target) in &mut tasks {
        let page = target.map(|target| target.entity());
        let Some((manifest, credentials_observed)) =
            future::block_on(future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(McpSnapshotTask, OperationRetained)>();
        commands.trigger(OperationCompleted {
            entity,
            result: manifest.as_ref().map(|_| ()).map_err(Clone::clone),
        });
        let current = credentials.single().ok();
        if current != Some(task.owner) || task.sequence < version.current {
            if let Some(page) = page
                && let Ok(mut state) = states.get_mut(page)
                && state.generation == task.generation
            {
                state.loading = false;
                state.pending = false;
                state.loaded = current == Some(task.owner) && version.valid;
                if let Some(result) = &task.result {
                    commands.entity(page).insert(result.clone());
                }
            }
            continue;
        }
        version.current = task.sequence;
        version.owner = Some(task.owner);
        for mut state in &mut states {
            if state.owner == Some(task.owner) {
                state.set_changed();
            }
        }
        let manifest = match manifest {
            Ok(manifest) => {
                version.valid = true;
                manifest
            }
            Err(error) => {
                version.valid = false;
                if let Some(page) = page
                    && let Ok(mut state) = states.get_mut(page)
                    && state.generation == task.generation
                {
                    state.loading = false;
                    state.pending = false;
                    commands.entity(page).insert(McpCompletion {
                        id: String::new(),
                        disconnect: false,
                        success: false,
                        message: error,
                    });
                }
                continue;
            }
        };
        materialize_mcp_observations(
            (Ok(manifest), credentials_observed),
            &catalog,
            &extras,
            &mut commands,
        );
        for mut state in &mut states {
            if state.owner == Some(task.owner) {
                state.set_changed();
            }
        }
        if let Some(page) = page
            && let Ok(mut state) = states.get_mut(page)
            && state.generation == task.generation
        {
            state.loading = false;
            state.pending = false;
            state.loaded = true;
            if let Some(result) = &task.result {
                commands.entity(page).insert(result.clone());
            }
        }
    }
}

fn materialize_mcp_observations(
    records: SnapshotRecords,
    catalog: &Query<(Entity, &Name, &McpCatalogServer)>,
    extras: &Query<Entity, With<McpConfiguredServer>>,
    commands: &mut Commands,
) {
    let (manifest, credentials_observed) = records;
    let manifest = manifest.unwrap();
    for old in extras {
        commands.entity(old).despawn();
    }
    let mut catalog_ids = BTreeSet::new();
    for (server_entity, _, server) in catalog {
        catalog_ids.insert(server.id.clone());
        let occupied = manifest.mcp.servers.contains_key(&server.id);
        let owned = manifest
            .mcp
            .servers
            .get(&server.id)
            .is_some_and(|configured| server.owns(configured));
        let mut authenticated = false;
        let mut readable = false;
        for (id, observed) in &credentials_observed {
            if id != &server.id {
                continue;
            }
            readable = observed.is_ok();
            if let Ok(Some(observed)) = observed {
                authenticated = owned && observed.authorizes(&server.url);
            }
        }
        commands.entity(server_entity).insert(McpServerFacts {
            occupied,
            owned,
            authenticated,
            readable,
        });
    }
    for (id, server) in manifest.mcp.servers {
        if catalog_ids.contains(&id) || id == "vmux" {
            continue;
        }
        let description = server
            .url
            .or(server.command)
            .unwrap_or_else(|| format!("{:?}", server.transport).to_ascii_lowercase());
        commands.spawn((
            Name::new(id.clone()),
            McpConfiguredServer { id, description },
        ));
    }
}

fn publish_mcp_connections(
    states: Query<
        (
            Entity,
            &McpPageState,
            Option<&EntityTarget<McpConnectionOperation>>,
            Option<&PendingMcpSnapshot>,
            Option<&McpCompletion>,
        ),
        Changed<McpPageState>,
    >,
    projection: McpServerProjection,
    operations: Query<(Option<&PendingMcpOperation>, Option<&auth::Source>)>,
    snapshots: Query<(&McpSnapshotTask, &OperationTarget)>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for (target, state, binding, snapshot, result) in &states {
        if !browsers.can_emit_to(&target) {
            continue;
        }
        let mut entries = Vec::new();
        if projection.version.valid && state.owner == projection.version.owner {
            for (name, server, facts) in &projection.servers {
                let Some(facts) = facts else {
                    continue;
                };
                let status = if !facts.readable {
                    McpServerStatus::Failed
                } else if facts.occupied && !facts.owned {
                    McpServerStatus::Configured
                } else if facts.owned && facts.authenticated {
                    McpServerStatus::Connected
                } else if facts.owned {
                    McpServerStatus::AuthenticationRequired
                } else {
                    McpServerStatus::Available
                };
                entries.push(McpServerEntry {
                    id: server.id.clone(),
                    name: name.as_str().to_owned(),
                    description: String::new(),
                    status,
                });
            }
            for (name, server) in &projection.extras {
                entries.push(McpServerEntry {
                    id: server.id.clone(),
                    name: name.as_str().to_owned(),
                    description: server.description.clone(),
                    status: McpServerStatus::Configured,
                });
            }
        }
        let mut pending = None;
        if let Some(binding) = binding
            && let Ok((request, task)) = operations.get(binding.entity())
        {
            pending = request
                .map(|request| (&request.id, request.disconnect))
                .or_else(|| task.map(|task| (&task.id, task.disconnect)));
        }
        if pending.is_none() && state.pending {
            if let Some(result) = snapshot.and_then(|snapshot| snapshot.result.as_ref()) {
                pending = Some((&result.id, result.disconnect));
            }
            for (snapshot, page) in &snapshots {
                if page.entity() == target
                    && let Some(result) = &snapshot.result
                {
                    pending = Some((&result.id, result.disconnect));
                }
            }
        }
        let pending = pending.map(|(id, disconnect)| McpServerPending {
            id: id.clone(),
            operation: if disconnect {
                McpServerOperation::Disconnect
            } else {
                McpServerOperation::Connect
            },
        });
        let result = result.map(|result| McpServerResult {
            id: result.id.clone(),
            operation: if result.disconnect {
                McpServerOperation::Disconnect
            } else {
                McpServerOperation::Connect
            },
            success: result.success,
            message: result.message.clone(),
        });
        commands.trigger(UiStateWrite::<McpServersUiState>::from_event(
            target,
            &McpServersUiState {
                loaded: state.loaded
                    && projection.version.valid
                    && state.owner == projection.version.owner,
                loading: state.loading,
                servers: entries,
                pending,
                result,
            },
        ));
    }
}

#[derive(EntityEvent)]
pub(crate) struct McpSnapshotRequest {
    #[event_target]
    pub target: Entity,
}

#[derive(Component, Default)]
struct McpPageState {
    generation: u64,
    owner: Option<Entity>,
    loaded: bool,
    loading: bool,
    pending: bool,
}

#[derive(Component, Default)]
struct McpServerClaim {
    operation: Option<Entity>,
}

#[derive(Component)]
struct McpServerFacts {
    occupied: bool,
    owned: bool,
    authenticated: bool,
    readable: bool,
}

#[derive(Component)]
struct McpConfiguredServer {
    id: String,
    description: String,
}

#[derive(Resource, Default)]
struct McpObservationVersion {
    next: u64,
    current: u64,
    owner: Option<Entity>,
    valid: bool,
}

#[derive(Component, Clone)]
struct McpCompletion {
    id: String,
    disconnect: bool,
    success: bool,
    message: String,
}

#[derive(Component)]
struct McpAuthorizationUrl {
    generation: u64,
    url: String,
}

#[derive(Component)]
struct McpConnectionOperation;

#[derive(Component)]
struct PendingMcpOperation {
    generation: u64,
    owner: Entity,
    id: String,
    disconnect: bool,
}

#[derive(Component, Clone)]
struct PendingMcpSnapshot {
    generation: u64,
    result: Option<McpCompletion>,
}

type SnapshotRecords = (
    Result<ToolsManifest, String>,
    Vec<(String, Result<Option<McpOauthCredentials>, String>)>,
);

#[derive(Component)]
struct McpSnapshotTask {
    generation: u64,
    sequence: u64,
    owner: Entity,
    result: Option<McpCompletion>,
    task: Task<SnapshotRecords>,
}

#[derive(Component, Clone)]
struct McpCatalogServer {
    id: String,
    url: String,
    scopes: Vec<String>,
}

impl McpCatalogServer {
    fn manifest(&self) -> McpServerManifest {
        McpServerManifest {
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: Default::default(),
            cwd: None,
            url: Some(self.url.clone()),
            headers: Default::default(),
            header_env: Default::default(),
            bearer_token_env_var: None,
        }
    }

    fn owns(&self, server: &McpServerManifest) -> bool {
        server == &self.manifest()
    }
}

fn spawn_mcp_catalog(features: Query<&FeatureManifest>, mut commands: Commands) {
    let mut ids = BTreeSet::new();
    for server in features.iter().flat_map(|feature| &feature.mcp_servers) {
        assert!(ids.insert(server.id.clone()), "duplicate MCP server ID");
        commands.spawn((
            Name::new(server.name.clone()),
            McpServerClaim::default(),
            McpCatalogServer {
                id: server.id.clone(),
                url: server.url.clone(),
                scopes: server.scopes.clone(),
            },
        ));
    }
}

#[derive(SystemParam)]
struct McpServerProjection<'w, 's> {
    servers: Query<
        'w,
        's,
        (
            &'static Name,
            &'static McpCatalogServer,
            Option<&'static McpServerFacts>,
        ),
    >,
    extras: Query<'w, 's, (&'static Name, &'static McpConfiguredServer)>,
    version: Res<'w, McpObservationVersion>,
}

#[derive(SystemParam)]
struct McpCatalog<'w, 's> {
    servers: Query<'w, 's, (&'static Name, &'static McpCatalogServer)>,
}

#[cfg(test)]
mod tests {
    use super::McpConnectionOperation;
    use super::transport::{Http, McpCallback};
    use super::{McpPageState, PendingMcpOperation};
    use bevy::prelude::*;
    use bevy::tasks::{IoTaskPool, TaskPoolBuilder};
    use bevy_cef::prelude::UiInput;
    use vmux_api::mcp::McpServerRequest;
    use vmux_ecs::EntityTarget;
    use vmux_ecs::{Operation, OperationRetained, OperationTarget};

    #[test]
    fn oauth_callback_requires_the_matching_state() {
        assert_eq!(
            McpCallback::code("/callback?code=abc&state=expected", "expected").unwrap(),
            "abc"
        );
        assert!(McpCallback::code("/callback?code=abc&state=wrong", "expected").is_err());
    }

    #[test]
    fn oauth_endpoints_require_https() {
        assert!(Http::https_url("http://example.com", "endpoint").is_err());
        assert!(Http::https_url("https://example.com", "endpoint").is_ok());
    }

    #[test]
    fn authorization_metadata_preserves_the_issuer_path() {
        assert_eq!(
            Http::well_known_url(
                "https://auth.example.com/tenant",
                "issuer",
                "oauth-authorization-server"
            )
            .unwrap()
            .as_str(),
            "https://auth.example.com/.well-known/oauth-authorization-server/tenant"
        );
    }

    #[test]
    fn admission_uses_shared_server_facts_and_refuses_foreign_or_unreadable_slots() {
        for (occupied, owned, authenticated, readable, expected) in [
            (false, false, false, true, Some(false)),
            (true, true, true, true, Some(true)),
            (true, true, false, true, Some(false)),
            (true, false, false, true, None),
            (true, true, true, false, None),
        ] {
            let mut app = App::new();
            app.add_plugins(super::McpConnectionPlugin);
            let owner = app.world_mut().spawn_empty().id();
            {
                let mut version = app
                    .world_mut()
                    .resource_mut::<super::McpObservationVersion>();
                version.owner = Some(owner);
                version.valid = true;
            }
            app.world_mut().spawn((
                Name::new("Linear"),
                super::McpCatalogServer {
                    id: "linear".into(),
                    url: "https://mcp.linear.app/mcp".into(),
                    scopes: Vec::new(),
                },
                super::McpServerClaim::default(),
                super::McpServerFacts {
                    occupied,
                    owned,
                    authenticated,
                    readable,
                },
            ));
            let target = app
                .world_mut()
                .spawn(McpPageState {
                    generation: 1,
                    owner: Some(owner),
                    loaded: true,
                    ..Default::default()
                })
                .id();
            app.world_mut().trigger(UiInput {
                webview: target,
                payload: McpServerRequest {
                    id: "linear".into(),
                },
            });
            app.world_mut().trigger(UiInput {
                webview: target,
                payload: McpServerRequest {
                    id: "linear".into(),
                },
            });
            app.world_mut().flush();
            let operation = app
                .world()
                .get::<EntityTarget<McpConnectionOperation>>(target)
                .map(|target| target.entity());
            let queued =
                operation.and_then(|entity| app.world().get::<PendingMcpOperation>(entity));
            assert_eq!(queued.map(|operation| operation.disconnect), expected);
            if let Some(queued) = queued {
                assert_eq!(queued.generation, 2);
                assert_eq!(queued.owner, owner);
                assert_eq!(
                    app.world().get::<McpPageState>(target).unwrap().generation,
                    2
                );
            }
        }
    }
    #[test]
    fn one_server_binding_refuses_a_second_page_admission() {
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        let owner = app.world_mut().spawn_empty().id();
        {
            let mut version = app
                .world_mut()
                .resource_mut::<super::McpObservationVersion>();
            version.owner = Some(owner);
            version.valid = true;
        }
        app.world_mut().spawn((
            Name::new("Linear"),
            super::McpCatalogServer {
                id: "linear".into(),
                url: "https://mcp.linear.app/mcp".into(),
                scopes: Vec::new(),
            },
            super::McpServerClaim::default(),
            super::McpServerFacts {
                occupied: false,
                owned: false,
                authenticated: false,
                readable: true,
            },
        ));
        let first = app
            .world_mut()
            .spawn(McpPageState {
                generation: 1,
                owner: Some(owner),
                ..Default::default()
            })
            .id();
        let second = app
            .world_mut()
            .spawn(McpPageState {
                generation: 1,
                owner: Some(owner),
                ..Default::default()
            })
            .id();
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: McpServerRequest {
                id: "linear".into(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview: second,
            payload: McpServerRequest {
                id: "linear".into(),
            },
        });
        app.world_mut().flush();
        assert!(
            app.world()
                .get::<EntityTarget<McpConnectionOperation>>(first)
                .is_some()
        );
        assert!(
            app.world()
                .get::<EntityTarget<McpConnectionOperation>>(second)
                .is_none()
        );
    }

    #[test]
    fn snapshot_from_an_old_profile_cannot_replace_current_global_observations() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        let old_owner = app.world_mut().spawn_empty().id();
        let current_owner = app
            .world_mut()
            .spawn(vmux_ecs::profile::McpCredentials {
                access: vmux_profile::mcp_credentials::McpCredentialAccess::default(),
                storage: vmux_profile::mcp_credentials::McpCredentialStorage::at(
                    vmux_profile::Profile::named("mcp-snapshot-contract"),
                    vmux_profile::ProfilePaths::current(),
                ),
            })
            .id();
        {
            let mut version = app
                .world_mut()
                .resource_mut::<super::McpObservationVersion>();
            version.current = 2;
            version.owner = Some(current_owner);
            version.valid = true;
        }
        let page = app
            .world_mut()
            .spawn(McpPageState {
                generation: 1,
                owner: Some(old_owner),
                loading: true,
                ..Default::default()
            })
            .id();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.snapshot").unwrap().running(),
                OperationRetained,
                OperationTarget::new(page),
                super::McpSnapshotTask {
                    generation: 1,
                    sequence: 1,
                    owner: old_owner,
                    result: None,
                    task: IoTaskPool::get()
                        .spawn(async move { (Ok(crate::ToolsManifest::default()), Vec::new()) }),
                },
            ))
            .id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app
            .world()
            .get::<super::McpSnapshotTask>(operation)
            .is_some()
        {
            app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "old snapshot did not drain"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let version = app.world().resource::<super::McpObservationVersion>();
        assert_eq!(version.current, 2);
        assert_eq!(version.owner, Some(current_owner));
        assert!(version.valid);
        assert!(!app.world().get::<McpPageState>(page).unwrap().loading);
        assert!(app.world().get::<OperationRetained>(operation).is_none());
    }
}
