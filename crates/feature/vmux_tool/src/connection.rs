use std::collections::BTreeSet;

use snapshot::SnapshotContext;

use crate::{McpServerManifest, McpTransport};
use bevy::app::{App, Plugin};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_api::mcp::{
    McpServerEntry, McpServerOperation, McpServerPending, McpServerRequest, McpServerResult,
    McpServerStatus, McpServersRequest, McpServersUiState,
};
use vmux_ecs::{EntityTarget, PageOpenRequest, PageOpenTarget};
use vmux_ecs::{
    Operation, OperationPlugin, OperationRetained, OperationTarget, UiStatePlugin, UiStateWrite,
};
use vmux_feature::manifest::FeatureManifest;

mod auth;
mod remove;
mod snapshot;
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
        if !app.is_plugin_added::<crate::storage::StoragePlugin>() {
            app.add_plugins(crate::storage::StoragePlugin);
        }
        app.add_plugins((
            auth::AuthPlugin,
            remove::RemovePlugin,
            snapshot::SnapshotPlugin,
        ))
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
            (publish_mcp_authorization, retire_mcp_operations)
                .chain()
                .after(auth::AuthSet)
                .after(remove::RemoveSet)
                .before(snapshot::SnapshotSet),
        )
        .add_systems(Update, publish_mcp_connections.after(snapshot::SnapshotSet));
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
    credentials: Query<(Entity, Ref<vmux_ecs::profile::McpCredentials>)>,
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
        || !credentials.single().is_ok_and(|(owner, source)| {
            Some(owner) == version.owner && Some(source.last_changed()) == version.source_tick
        })
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
    snapshots: Query<(&SnapshotContext, &OperationTarget)>,
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
        commands.trigger(McpServersSnapshot {
            target,
            loading: state.loading,
            has_pending: pending.is_some(),
            servers: entries.clone(),
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
pub(crate) struct McpServersSnapshot {
    #[event_target]
    pub target: Entity,
    pub loading: bool,
    pub has_pending: bool,
    pub servers: Vec<McpServerEntry>,
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

#[derive(Component, Clone, Default)]
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
    source_tick: Option<bevy::ecs::change_detection::Tick>,
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

    fn authorizes(&self, resource: &str) -> bool {
        let Ok(stored) = url::Url::parse(resource) else {
            return false;
        };
        let Ok(requested) = url::Url::parse(&self.url) else {
            return false;
        };
        stored.scheme() == "https" && requested.scheme() == "https" && stored == requested
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

#[cfg(test)]
mod tests {
    use super::McpConnectionOperation;
    use super::transport::{Http, McpCallback};
    use super::{McpPageState, PendingMcpOperation};
    use bevy::prelude::*;
    use bevy_cef::prelude::UiInput;
    use vmux_api::mcp::McpServerRequest;
    use vmux_ecs::EntityTarget;

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
            let directory = tempfile::tempdir().unwrap();
            let profile = vmux_profile::Profile::named("admission-contract");
            let owner = app
                .world_mut()
                .spawn(vmux_ecs::profile::McpCredentials {
                    access: vmux_profile::mcp_credentials::McpCredentialAccess::default(),
                    storage: vmux_profile::mcp_credentials::McpCredentialStorage::at(
                        profile.clone(),
                        vmux_profile::ProfilePaths::at(profile, directory.path().to_path_buf()),
                    ),
                })
                .id();
            let tick = app
                .world()
                .entity(owner)
                .get_ref::<vmux_ecs::profile::McpCredentials>()
                .unwrap()
                .last_changed();
            {
                let mut version = app
                    .world_mut()
                    .resource_mut::<super::McpObservationVersion>();
                version.owner = Some(owner);
                version.source_tick = Some(tick);
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
        let directory = tempfile::tempdir().unwrap();
        let profile = vmux_profile::Profile::named("admission-contract");
        let owner = app
            .world_mut()
            .spawn(vmux_ecs::profile::McpCredentials {
                access: vmux_profile::mcp_credentials::McpCredentialAccess::default(),
                storage: vmux_profile::mcp_credentials::McpCredentialStorage::at(
                    profile.clone(),
                    vmux_profile::ProfilePaths::at(profile, directory.path().to_path_buf()),
                ),
            })
            .id();
        let tick = app
            .world()
            .entity(owner)
            .get_ref::<vmux_ecs::profile::McpCredentials>()
            .unwrap()
            .last_changed();
        {
            let mut version = app
                .world_mut()
                .resource_mut::<super::McpObservationVersion>();
            version.owner = Some(owner);
            version.source_tick = Some(tick);
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
}
