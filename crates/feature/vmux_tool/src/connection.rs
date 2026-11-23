use std::collections::BTreeSet;

use bevy::app::{App, Plugin};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_api::mcp::{
    McpAuthorization, McpLinkOpenRequest, McpServerCancelRequest, McpServerEntry,
    McpServerOperation, McpServerPending, McpServerRequest, McpServerResult, McpServerStatus,
    McpServersRequest, McpServersUiState,
};
use vmux_ecs::{
    EntityTarget, Operation, OperationPlugin, OperationRetained, OperationTarget, PageOpenRequest,
    PageOpenTarget, UiStatePlugin, UiStateWrite,
};
use vmux_feature::manifest::FeatureManifest;

use crate::McpServerManifest;
use snapshot::SnapshotContext;
use transport::Http;

mod auth;
mod remove;
mod snapshot;
mod transport;

pub struct McpConnectionPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConnectionSet;

#[derive(Event)]
pub struct ConnectorConnectionCompleted(pub McpServerResult);

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
            UiEventPlugin::<(
                McpServersRequest,
                McpServerRequest,
                McpServerCancelRequest,
                McpLinkOpenRequest,
            )>::default(),
            UiStatePlugin::<McpServersUiState>::default(),
        ))
        .add_systems(Startup, spawn_mcp_catalog)
        .add_message::<PageOpenRequest>()
        .add_message::<vmux_ecs::browser::BrowserOpenRequest>()
        .add_observer(cancel)
        .add_observer(open_link)
        .add_observer(page_ready)
        .add_observer(dispose_setup)
        .add_observer(request_mcp_connections)
        .add_observer(begin_mcp_snapshot)
        .add_observer(request_mcp_server)
        .add_systems(
            Update,
            (
                publish_mcp_authorization,
                retire_mcp_operations,
                start_links,
                finish_links,
            )
                .chain()
                .in_set(ConnectionSet)
                .after(auth::AuthSet)
                .after(remove::RemoveSet)
                .before(snapshot::SnapshotSet),
        )
        .add_systems(
            Update,
            publish_mcp_connections
                .in_set(ConnectionSet)
                .after(snapshot::SnapshotSet),
        );
    }
}

#[derive(Component)]
struct ExternalLink(String);

#[derive(Component)]
struct ExternalLinkTask(Task<Result<(), String>>);

struct SystemBrowser;

impl SystemBrowser {
    fn open(url: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        let program = "open";
        #[cfg(not(target_os = "macos"))]
        let program = "xdg-open";
        let status = std::process::Command::new(program)
            .arg(url)
            .status()
            .map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("System browser failed ({status})"));
        }
        Ok(())
    }
}

fn open_link(
    trigger: On<UiInput<McpLinkOpenRequest>>,
    states: Query<&McpPageState>,
    servers: Query<&McpCatalogServer>,
    mut opens: MessageWriter<PageOpenRequest>,
    mut browsers: MessageWriter<vmux_ecs::browser::BrowserOpenRequest>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let url = &event.payload.url;
    if Http::https_url(url, "connector link").is_err() {
        return;
    }
    let sign_in = states.get(event.webview).is_ok_and(|state| {
        state
            .authorization
            .as_ref()
            .is_some_and(|authorization| &authorization.url == url)
    });
    let setup = servers
        .iter()
        .any(|server| server.setup_url.as_ref() == Some(url));
    if !sign_in && !setup {
        return;
    }
    if event.payload.external {
        commands.spawn((
            Name::new("Open connector link externally"),
            ExternalLink(url.clone()),
            ChildOf(event.webview),
        ));
    } else if sign_in {
        browsers.write(vmux_ecs::browser::BrowserOpenRequest {
            source: event.webview,
            url: url.clone(),
        });
    } else {
        opens.write(PageOpenRequest {
            target: PageOpenTarget::NewStack,
            url: url.clone(),
            request_id: None,
        });
    }
}

fn start_links(
    requests: Query<(Entity, &ExternalLink), Without<ExternalLinkTask>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, link) in &requests {
        let url = link.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        commands
            .entity(entity)
            .insert(ExternalLinkTask(IoTaskPool::get().spawn(async move {
                let result = SystemBrowser::open(&url);
                if let Some(wake) = wake {
                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                }
                result
            })));
    }
}

fn finish_links(mut tasks: Query<(Entity, &mut ExternalLinkTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        if let Err(error) = result {
            warn!("Failed to open external connector link: {error}");
        }
        commands.entity(entity).despawn();
    }
}

#[derive(Component)]
struct SetupOwner(vmux_api::mcp::ConnectorSetupRoute);

fn cancel(
    trigger: On<UiInput<McpServerCancelRequest>>,
    mut states: Query<(
        &mut McpPageState,
        Option<&EntityTarget<McpConnectionOperation>>,
    )>,
    operations: Query<(Option<&PendingMcpOperation>, Option<&auth::Source>)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    if let Ok((mut state, binding)) = states.get_mut(target) {
        if let Some(binding) = binding {
            let operation = binding.entity();
            if let Ok((pending, source)) = operations.get(operation) {
                let id_and_disconnect = pending
                    .map(|request| (request.id.clone(), request.disconnect))
                    .or_else(|| source.map(|source| (source.id.clone(), source.disconnect)));
                if let Some((id, disconnect)) = id_and_disconnect {
                    commands.trigger(ConnectorConnectionCompleted(McpServerResult {
                        id,
                        operation: if disconnect {
                            McpServerOperation::Disconnect
                        } else {
                            McpServerOperation::Connect
                        },
                        success: false,
                        message: String::new(),
                    }));
                }
            }
            commands.entity(operation).insert(auth::Canceled);
        }
        state.pending = false;
        state.authorization = None;
        commands
            .entity(target)
            .remove::<(EntityTarget<McpConnectionOperation>, McpCompletion)>();
    }
    commands.trigger(McpSnapshotRequest { target });
}

fn page_ready(
    trigger: On<UiInput<vmux_api::PageReady>>,
    pages: Query<&vmux_ecs::PageMetadata>,
    owners: Query<&SetupOwner>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok(page) = pages.get(target) else {
        return;
    };
    let setup = vmux_api::mcp::ConnectorSetupRoute::parse(&page.url);
    if owners
        .get(target)
        .is_ok_and(|owner| Some(&owner.0) != setup.as_ref())
    {
        if let Ok(owner) = owners.get(target) {
            commands.trigger(ConnectorConnectionCompleted(McpServerResult {
                id: owner.0.0.clone(),
                operation: McpServerOperation::Connect,
                success: false,
                message: String::new(),
            }));
        }
        commands.trigger(UiInput {
            webview: target,
            payload: McpServerCancelRequest,
        });
        commands.entity(target).remove::<SetupOwner>();
    }
    if let Some(setup) = setup {
        commands.entity(target).insert(SetupOwner(setup));
    }
    if vmux_api::RoutePattern::new(crate::CONNECTORS_URL, true).matches(&page.url) {
        commands.trigger(McpSnapshotRequest { target });
    }
}

fn dispose_setup(
    trigger: On<Despawn, SetupOwner>,
    owners: Query<&SetupOwner>,
    bindings: Query<&EntityTarget<McpConnectionOperation>>,
    mut commands: Commands,
) {
    if let Ok(binding) = bindings.get(trigger.entity) {
        commands.entity(binding.entity()).insert(auth::Canceled);
    }
    if let Ok(owner) = owners.get(trigger.entity) {
        commands.trigger(ConnectorConnectionCompleted(McpServerResult {
            id: owner.0.0.clone(),
            operation: McpServerOperation::Connect,
            success: false,
            message: String::new(),
        }));
    }
}

fn request_mcp_connections(trigger: On<UiInput<McpServersRequest>>, mut commands: Commands) {
    commands.trigger(McpSnapshotRequest {
        target: trigger.event().webview,
    });
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
    let Some((server_entity, server, mut claim)) = selected else {
        return;
    };
    if !server.readable || server.occupied && !server.owned {
        return;
    }
    let disconnect = server.owned && server.authenticated;
    state.generation = state.generation.wrapping_add(1).max(1);
    state.pending = true;
    state.authorization = None;
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
            EntityTarget::<McpCatalogServer>::new(server_entity),
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
    mut states: Query<(&mut McpPageState, &EntityTarget<McpConnectionOperation>)>,
    mut browsers: MessageWriter<vmux_ecs::browser::BrowserOpenRequest>,
    mut commands: Commands,
) {
    for (entity, url, target) in &urls {
        commands.entity(entity).remove::<McpAuthorizationUrl>();
        let Some(target) = target else {
            continue;
        };
        let page = target.entity();
        let Ok((mut state, binding)) = states.get_mut(page) else {
            continue;
        };
        if state.generation != url.generation || binding.entity() != entity {
            continue;
        }
        state.authorization = Some(url.authorization.clone());
        browsers.write(vmux_ecs::browser::BrowserOpenRequest {
            source: page,
            url: url.authorization.url.clone(),
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
    mut states: Query<(
        Entity,
        &mut McpPageState,
        Option<&EntityTarget<McpConnectionOperation>>,
    )>,
    mut servers: Query<&mut McpServerClaim, With<McpCatalogServer>>,
    mut commands: Commands,
) {
    for (entity, result, target, server) in &done {
        if let Ok(mut claim) = servers.get_mut(server.entity())
            && claim.operation == Some(entity)
        {
            claim.operation = None;
        }
        let mut refreshed_source = None;
        if let Some(target) = target
            && let Ok((_, mut state, Some(binding))) = states.get_mut(target.entity())
            && binding.entity() == entity
        {
            state.authorization = None;
            state.loading = true;
            refreshed_source = Some(target.entity());
            let completed = McpServerResult {
                id: result.id.clone(),
                operation: if result.disconnect {
                    McpServerOperation::Disconnect
                } else {
                    McpServerOperation::Connect
                },
                success: result.success,
                message: result.message.clone(),
            };
            commands.trigger(ConnectorConnectionCompleted(completed));
            commands
                .entity(target.entity())
                .remove::<EntityTarget<McpConnectionOperation>>();
            commands.entity(target.entity()).insert(PendingMcpSnapshot {
                generation: state.generation,
                result: Some(result.clone()),
            });
        }
        if result.success {
            for (page, _, _) in &states {
                if Some(page) != refreshed_source {
                    commands.trigger(McpSnapshotRequest { target: page });
                }
            }
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
    for (target, state, binding, snapshot, result) in &states {
        if let Some(browsers) = browsers.as_deref()
            && !browsers.can_emit_to(&target)
        {
            continue;
        }
        let entries = projection.entries(state.owner);
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
                selected: Vec::new(),
                loaded: state.loaded
                    && projection.version.valid
                    && state.owner == projection.version.owner,
                loading: state.loading,
                servers: entries,
                pending,
                result,
                authorization: state.authorization.clone(),
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
    authorization: Option<McpAuthorization>,
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
    account: Option<String>,
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
    authorization: McpAuthorization,
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
    setup_url: Option<String>,
    client_id: Option<String>,
    client_id_env: Option<String>,
    account_endpoint: Option<String>,
    id: String,
    url: String,
    scopes: Vec<String>,
}

impl McpCatalogServer {
    fn manifest(&self) -> McpServerManifest {
        McpServerManifest::http(&self.url)
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
                setup_url: server.setup_url.clone(),
                client_id: server.client_id.clone(),
                client_id_env: server.client_id_env.clone(),
                account_endpoint: server.account_endpoint.clone(),
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

impl McpServerProjection<'_, '_> {
    fn entries(&self, owner: Option<Entity>) -> Vec<McpServerEntry> {
        let mut entries = Vec::new();
        if !self.version.valid || owner != self.version.owner {
            return entries;
        }
        for (name, server, facts) in &self.servers {
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
                setup_url: server.setup_url.clone(),
                account: facts.account.clone(),
                id: server.id.clone(),
                name: name.as_str().to_owned(),
                description: String::new(),
                status,
            });
        }
        for (name, server) in &self.extras {
            entries.push(McpServerEntry {
                setup_url: None,
                account: None,
                id: server.id.clone(),
                name: name.as_str().to_owned(),
                description: server.description.clone(),
                status: McpServerStatus::Configured,
            });
        }
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::McpConnectionOperation;
    use super::transport::{DeviceToken, Http, McpCallback};
    use super::{McpPageState, PendingMcpOperation};
    use bevy::prelude::*;
    use bevy_cef::prelude::UiInput;
    use vmux_api::mcp::{McpAuthorization, McpLinkOpenRequest, McpServerRequest};
    use vmux_ecs::EntityTarget;
    use vmux_feature::manifest::FeatureManifest;

    #[test]
    fn connector_links_open_in_app_and_queue_external_opening_only_on_request() {
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        app.world_mut()
            .spawn(FeatureManifest::of::<crate::Feature>().clone());
        app.update();
        let url = "https://github.com/login/device";
        let target = app
            .world_mut()
            .spawn(McpPageState {
                generation: 1,
                pending: true,
                authorization: Some(McpAuthorization {
                    url: url.into(),
                    code: Some("ABCD-1234".into()),
                }),
                ..Default::default()
            })
            .id();
        app.world_mut().trigger(UiInput {
            webview: target,
            payload: McpLinkOpenRequest {
                url: url.into(),
                external: false,
            },
        });
        app.world_mut().flush();
        let opens = app
            .world_mut()
            .resource_mut::<Messages<vmux_ecs::browser::BrowserOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opens.len(), 1);
        assert!(opens[0].source == target && opens[0].url == url);
        app.world_mut().trigger(UiInput {
            webview: target,
            payload: McpLinkOpenRequest {
                url: url.into(),
                external: true,
            },
        });
        app.world_mut().flush();
        let mut links = app.world_mut().query::<(&super::ExternalLink, &ChildOf)>();
        let requested = links.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(requested.len(), 1);
        assert_eq!(requested[0].0.0, url);
        assert_eq!(requested[0].1.parent(), target);
        assert!(
            app.world()
                .resource::<Messages<super::PageOpenRequest>>()
                .is_empty()
        );
    }

    #[test]
    fn connector_links_reject_unpublished_urls_and_non_https_schemes() {
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        let target = app.world_mut().spawn(McpPageState::default()).id();
        for url in [
            "https://example.com",
            "file:///tmp/private",
            "javascript:alert(1)",
        ] {
            app.world_mut().trigger(UiInput {
                webview: target,
                payload: McpLinkOpenRequest {
                    url: url.into(),
                    external: false,
                },
            });
        }
        app.world_mut().flush();
        assert!(
            app.world()
                .resource::<Messages<super::PageOpenRequest>>()
                .is_empty()
        );
        let mut links = app.world_mut().query::<&super::ExternalLink>();
        assert_eq!(links.iter(app.world()).count(), 0);
    }

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
    fn device_authorization_waits_slows_down_and_stops_on_denial_or_expiry() {
        let mut interval = 5;
        for error in ["authorization_pending", "slow_down"] {
            let response: DeviceToken =
                serde_json::from_value(serde_json::json!({"error": error})).unwrap();
            assert!(response.poll(&mut interval).unwrap().is_none());
        }
        assert_eq!(interval, 10);
        for (error, message) in [
            ("access_denied", "Authentication was denied"),
            ("expired_token", "Authentication timed out"),
        ] {
            let response: DeviceToken =
                serde_json::from_value(serde_json::json!({"error": error})).unwrap();
            assert_eq!(response.poll(&mut interval).err().as_deref(), Some(message));
        }
        let response: DeviceToken = serde_json::from_value(
            serde_json::json!({"access_token": "approved", "expires_in": 28800}),
        )
        .unwrap();
        let token = response.poll(&mut interval).unwrap().unwrap();
        assert_eq!(token.access_token, "approved");
        assert_eq!(token.expires_in, Some(28800));
    }

    #[test]
    fn snapshot_requests_are_retained_before_the_browser_is_ready() {
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        let directory = tempfile::tempdir().unwrap();
        let profile = vmux_profile::Profile::named("snapshot-before-browser");
        app.world_mut().spawn(vmux_ecs::profile::McpCredentials {
            access: vmux_profile::mcp_credentials::McpCredentialAccess::default(),
            storage: vmux_profile::mcp_credentials::McpCredentialStorage::at(
                profile.clone(),
                vmux_profile::ProfilePaths::at(profile, directory.path().to_path_buf()),
            ),
        });
        let target = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(UiInput {
            webview: target,
            payload: vmux_api::mcp::McpServersRequest,
        });
        app.world_mut().flush();
        assert!(app.world().get::<McpPageState>(target).unwrap().loading);
        assert!(
            app.world()
                .get::<super::PendingMcpSnapshot>(target)
                .is_some()
        );
    }

    #[test]
    fn only_connector_pages_load_account_connections_on_page_ready() {
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        for (url, should_load) in [
            ("vmux://tools/connectors", true),
            ("vmux://connectors/", true),
            ("vmux://tools/mcp", false),
            ("vmux://tools/acp", false),
        ] {
            let target = app
                .world_mut()
                .spawn(vmux_ecs::PageMetadata {
                    url: url.into(),
                    ..Default::default()
                })
                .id();
            app.world_mut().trigger(UiInput {
                webview: target,
                payload: vmux_api::PageReady {},
            });
            app.update();
            assert_eq!(
                app.world().get::<McpPageState>(target).is_some(),
                should_load,
                "{url}"
            );
        }
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
                    setup_url: None,
                    client_id: None,
                    client_id_env: None,
                    account_endpoint: None,
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
                    account: None,
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
                setup_url: None,
                client_id: None,
                client_id_env: None,
                account_endpoint: None,
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
                account: None,
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
