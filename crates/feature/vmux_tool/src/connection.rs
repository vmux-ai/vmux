use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::{McpServerManifest, McpTransport, ToolStore, ToolsManifest};
use bevy::app::{App, Plugin};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use parking_lot::Mutex;
use reqwest::blocking::{Client, Response};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use url::Url;
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

pub struct McpConnectionPlugin;

impl Plugin for McpConnectionPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.init_resource::<McpObservationVersion>()
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
                    start_mcp_operation,
                    drain_mcp_operations,
                    publish_mcp_authorization,
                    retire_mcp_operations,
                    start_mcp_snapshots,
                    drain_mcp_snapshots,
                    publish_mcp_connections,
                )
                    .chain(),
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

fn start_mcp_operation(
    pending: Query<
        (Entity, &PendingMcpOperation),
        (With<McpConnectionOperation>, Without<McpOperationTask>),
    >,
    catalog: McpCatalog,
    credentials: Query<(Entity, &McpCredentials)>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok((owner, credentials)) = credentials.single() else {
        return;
    };
    for (entity, pending) in &pending {
        if pending.owner != owner {
            commands.entity(entity).remove::<PendingMcpOperation>();
            commands.entity(entity).insert(McpCompletion {
                id: pending.id.clone(),
                disconnect: pending.disconnect,
                success: false,
                message: "MCP source Profile changed".into(),
            });
            commands.trigger(OperationCompleted {
                entity,
                result: Err("MCP source Profile changed".into()),
            });
            continue;
        }
        let generation = pending.generation;
        let id = pending.id.clone();
        let disconnect = pending.disconnect;
        let server = catalog.server(&id);
        let credentials = (*credentials).clone();
        let task_id = id.clone();
        let completion_wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let progress_wake = completion_wake.clone();
        let (progress_sender, progress_receiver) = mpsc::channel();
        let task = IoTaskPool::get().spawn(async move {
            let result = match (disconnect, server.as_ref()) {
                (false, Some(server)) => {
                    McpConnection::connect(server, &credentials, |url| {
                        if progress_sender.send(url).is_ok()
                            && let Some(wake) = &progress_wake
                        {
                            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                        }
                    })
                    .await
                }
                (true, Some(server)) => McpConnection::disconnect(server, &credentials).await,
                (_, None) => Err(format!("Unknown MCP server: {task_id}")),
            };
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(entity)
            .remove::<PendingMcpOperation>()
            .insert(McpOperationTask {
                generation,
                id,
                disconnect,
                task,
                progress: Mutex::new(progress_receiver),
            });
    }
}

fn drain_mcp_operations(mut tasks: Query<(Entity, &mut McpOperationTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        while let Ok(url) = task.progress.get_mut().try_recv() {
            commands.entity(entity).insert(McpAuthorizationUrl {
                generation: task.generation,
                url,
            });
        }
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        let (success, message) = match &result {
            Ok(()) => (true, String::new()),
            Err(error) => (false, error.clone()),
        };
        commands
            .entity(entity)
            .remove::<McpOperationTask>()
            .insert(McpCompletion {
                id: task.id.clone(),
                disconnect: task.disconnect,
                success,
                message,
            });
        commands.trigger(OperationCompleted { entity, result });
    }
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
            Without<McpOperationTask>,
            Without<PendingMcpOperation>,
        ),
    >,
    mut states: Query<(&mut McpPageState, &EntityTarget<McpConnectionOperation>)>,
    mut servers: Query<&mut McpServerClaim, With<McpCatalogServer>>,
    mut commands: Commands,
) {
    for (entity, result, target, server) in &done {
        if let Ok(mut claim) = servers.get_mut(server.entity())
            && claim.operation == Some(entity) {
                claim.operation = None;
            }
        if let Some(target) = target
            && let Ok((mut state, binding)) = states.get_mut(target.entity())
            && binding.entity() == entity {
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
                && state.generation == task.generation {
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
                    && state.generation == task.generation {
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
            && state.generation == task.generation {
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
    operations: Query<(Option<&PendingMcpOperation>, Option<&McpOperationTask>)>,
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
            && let Ok((request, task)) = operations.get(binding.entity()) {
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
                    && let Some(result) = &snapshot.result {
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

#[derive(Component)]
struct McpOperationTask {
    generation: u64,
    id: String,
    disconnect: bool,
    task: Task<Result<(), String>>,
    progress: Mutex<mpsc::Receiver<String>>,
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

impl McpCatalog<'_, '_> {
    fn server(&self, id: &str) -> Option<McpCatalogServer> {
        self.servers
            .iter()
            .find_map(|(_, server)| (server.id == id).then(|| server.clone()))
    }
}

struct McpConnection;

impl McpConnection {
    async fn connect(
        server: &McpCatalogServer,
        credentials: &McpCredentials,
        progress: impl FnOnce(String),
    ) -> Result<(), String> {
        {
            let store = ToolStore::current();
            let _lease = store.lease_async().await;
            Self::ensure_catalog_slot(&store, server)?;
        }
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("failed to open OAuth callback: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("failed to read OAuth callback address: {error}"))?;
        let redirect_uri = format!("http://127.0.0.1:{}/callback", address.port());
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?;
        let protected = Self::protected_metadata(&client, &server.url)?;
        let issuer = protected
            .authorization_servers
            .first()
            .ok_or_else(|| "MCP server did not advertise an authorization server".to_string())?;
        let authorization = Self::authorization_metadata(&client, issuer)?;
        let registration = Self::register(&client, &authorization, &redirect_uri)?;
        let verifier = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let challenge = base64_url(digest(&SHA256, verifier.as_bytes()).as_ref());
        let state = uuid::Uuid::new_v4().simple().to_string();
        let mut url = Self::https_url(
            &authorization.authorization_endpoint,
            "OAuth authorization endpoint",
        )?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &registration.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("resource", &server.url)
            .append_pair("scope", &server.scopes.join(" "));
        progress(url.to_string());
        let code = McpCallback::receive(listener, &state)?;
        let token = Self::exchange(
            &client,
            &authorization.token_endpoint,
            &registration,
            &code,
            &verifier,
            &redirect_uri,
            &server.url,
        )?;
        let oauth = McpOauthCredentials {
            token_endpoint: authorization.token_endpoint,
            client_id: registration.client_id,
            client_secret: registration.client_secret,
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at: McpOauthCredentials::expires_at(token.expires_in),
            scope: token.scope.unwrap_or_else(|| server.scopes.join(" ")),
            resource: server.url.clone(),
        };
        let id = &server.id;
        let store = ToolStore::current();
        let _lease = store.lease_async().await;
        credentials.access.write(|| {
            Self::ensure_catalog_slot(&store, server)?;
            let original_credentials = credentials.storage.load(id)?;
            credentials.storage.store(id, &oauth)?;
            if let Err(error) = Self::write_manifest(&store, server) {
                let credentials_rollback = original_credentials
                    .as_ref()
                    .map(|original| credentials.storage.store(id, original))
                    .unwrap_or_else(|| credentials.storage.remove(id));
                return Err(Self::rollback_error(error, Ok(()), credentials_rollback));
            }
            Ok(())
        })
    }

    async fn disconnect(
        server: &McpCatalogServer,
        credentials: &McpCredentials,
    ) -> Result<(), String> {
        let id = &server.id;
        let store = ToolStore::current();
        let _lease = store.lease_async().await;
        credentials.access.write(|| {
            let stored = credentials.storage.load(id)?;
            let original = store.load()?;
            let configured = original
                .mcp
                .servers
                .get(id)
                .ok_or_else(|| format!("MCP server is not configured: {id}"))?;
            if !server.owns(configured) {
                return Err(format!(
                    "MCP server ID is already managed by tools.toml: {id}"
                ));
            }
            let mut updated = original.clone();
            updated.mcp.servers.remove(id);
            store.save(&updated)?;
            if let Err(error) = credentials.storage.remove(id) {
                let manifest_rollback = store.save(&original);
                let credentials_rollback = stored
                    .as_ref()
                    .map(|stored| credentials.storage.store(id, stored))
                    .unwrap_or(Ok(()));
                return Err(Self::rollback_error(
                    error,
                    manifest_rollback,
                    credentials_rollback,
                ));
            }
            Ok(())
        })
    }

    fn protected_metadata(client: &Client, resource: &str) -> Result<ProtectedResource, String> {
        let metadata = Self::well_known_url(resource, "MCP resource", "oauth-protected-resource")?;
        Self::json(client.get(metadata).send())
    }

    fn authorization_metadata(
        client: &Client,
        issuer: &str,
    ) -> Result<AuthorizationServer, String> {
        let metadata = Self::well_known_url(issuer, "OAuth issuer", "oauth-authorization-server")?;
        Self::json(client.get(metadata).send())
    }

    fn register(
        client: &Client,
        authorization: &AuthorizationServer,
        redirect_uri: &str,
    ) -> Result<ClientRegistration, String> {
        let endpoint = authorization
            .registration_endpoint
            .as_ref()
            .ok_or_else(|| {
                "OAuth server does not support dynamic client registration".to_string()
            })?;
        let endpoint = Self::https_url(endpoint, "OAuth registration endpoint")?;
        Self::json(
            client
                .post(endpoint)
                .json(&RegistrationRequest {
                    client_name: "vmux",
                    redirect_uris: [redirect_uri],
                    grant_types: ["authorization_code", "refresh_token"],
                    response_types: ["code"],
                    token_endpoint_auth_method: "none",
                })
                .send(),
        )
    }

    fn exchange(
        client: &Client,
        endpoint: &str,
        registration: &ClientRegistration,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        resource: &str,
    ) -> Result<TokenResponse, String> {
        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("client_id", registration.client_id.clone()),
            ("redirect_uri", redirect_uri.to_string()),
            ("code_verifier", verifier.to_string()),
            ("resource", resource.to_string()),
        ];
        if let Some(secret) = &registration.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        let endpoint = Self::https_url(endpoint, "OAuth token endpoint")?;
        Self::json(client.post(endpoint).form(&form).send())
    }

    fn json<T: for<'de> Deserialize<'de>>(
        response: Result<Response, reqwest::Error>,
    ) -> Result<T, String> {
        let response = response.map_err(|error| error.to_string())?;
        let status = response.status();
        let body = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("OAuth request failed ({status}): {body}"));
        }
        serde_json::from_str(&body).map_err(|error| format!("invalid OAuth response: {error}"))
    }

    fn https_url(value: &str, label: &str) -> Result<Url, String> {
        let url = Url::parse(value).map_err(|error| format!("invalid {label}: {error}"))?;
        if url.scheme() != "https" {
            return Err(format!("{label} must use https"));
        }
        Ok(url)
    }

    fn well_known_url(value: &str, label: &str, metadata: &str) -> Result<Url, String> {
        let mut url = Self::https_url(value, label)?;
        let suffix = match url.path() {
            "/" => String::new(),
            path => path.to_string(),
        };
        url.set_path(&format!("/.well-known/{metadata}{suffix}"));
        url.set_query(None);
        url.set_fragment(None);
        Ok(url)
    }

    fn rollback_error(
        error: String,
        manifest: Result<(), String>,
        credentials: Result<(), String>,
    ) -> String {
        let mut failures = Vec::new();
        if let Err(error) = manifest {
            failures.push(format!("manifest rollback failed: {error}"));
        }
        if let Err(error) = credentials {
            failures.push(format!("credential rollback failed: {error}"));
        }
        if failures.is_empty() {
            return error;
        }
        format!("{error}; {}", failures.join("; "))
    }

    fn write_manifest(store: &ToolStore, server: &McpCatalogServer) -> Result<(), String> {
        let mut manifest = store.load()?;
        if let Some(manifest_server) = manifest.mcp.servers.get(&server.id)
            && !server.owns(manifest_server)
        {
            return Err(format!(
                "MCP server ID is already managed by tools.toml: {}",
                server.id
            ));
        }
        manifest
            .mcp
            .servers
            .insert(server.id.clone(), server.manifest());
        store.save(&manifest)
    }

    fn ensure_catalog_slot(store: &ToolStore, server: &McpCatalogServer) -> Result<(), String> {
        let manifest = store.load()?;
        let Some(manifest_server) = manifest.mcp.servers.get(&server.id) else {
            return Ok(());
        };
        if server.owns(manifest_server) {
            return Ok(());
        }
        Err(format!(
            "MCP server ID is already managed by tools.toml: {}",
            server.id
        ))
    }
}

struct McpCallback;

impl McpCallback {
    fn receive(listener: TcpListener, expected_state: &str) -> Result<String, String> {
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => return Self::read(&mut stream, expected_state),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err("OAuth authorization timed out".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(error) => return Err(format!("OAuth callback failed: {error}")),
            }
        }
    }

    fn read(stream: &mut TcpStream, expected_state: &str) -> Result<String, String> {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        const MAX_REQUEST_LINE_BYTES: u64 = 16 * 1024;
        let mut request = String::new();
        let mut reader = BufReader::new(&mut *stream).take(MAX_REQUEST_LINE_BYTES + 1);
        let length = reader
            .read_line(&mut request)
            .map_err(|error| format!("failed to read OAuth callback: {error}"))?;
        if length == 0 || length as u64 > MAX_REQUEST_LINE_BYTES || !request.ends_with('\n') {
            return Err("invalid OAuth callback".to_string());
        }
        let target = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .ok_or_else(|| "invalid OAuth callback".to_string())?;
        let result = Self::code(target, expected_state);
        let response = match result {
            Ok(_) => b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".as_slice(),
            Err(_) => b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n".as_slice(),
        };
        stream
            .write_all(response)
            .map_err(|error| format!("failed to finish OAuth callback: {error}"))?;
        result
    }

    fn code(target: &str, expected_state: &str) -> Result<String, String> {
        let url = Url::parse(&format!("http://127.0.0.1{target}"))
            .map_err(|error| format!("invalid OAuth callback: {error}"))?;
        let values = url
            .query_pairs()
            .collect::<std::collections::BTreeMap<_, _>>();
        let state = values
            .get("state")
            .ok_or_else(|| "OAuth callback omitted state".to_string())?;
        if state.as_ref() != expected_state {
            return Err("OAuth callback state did not match".to_string());
        }
        if let Some(error) = values.get("error") {
            return Err(values
                .get("error_description")
                .map(|description| description.to_string())
                .unwrap_or_else(|| error.to_string()));
        }
        let code = values
            .get("code")
            .ok_or_else(|| "OAuth callback omitted authorization code".to_string())?
            .to_string();
        Ok(code)
    }
}

#[derive(Deserialize)]
struct ProtectedResource {
    authorization_servers: Vec<String>,
}

#[derive(Deserialize)]
struct AuthorizationServer {
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
}

#[derive(Serialize)]
struct RegistrationRequest<'a> {
    client_name: &'static str,
    redirect_uris: [&'a str; 1],
    grant_types: [&'static str; 2],
    response_types: [&'static str; 1],
    token_endpoint_auth_method: &'static str,
}

#[derive(Deserialize)]
struct ClientRegistration {
    client_id: String,
    client_secret: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
}

fn base64_url(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or_default();
        let third = chunk.get(2).copied().unwrap_or_default();
        encoded.push(TABLE[(first >> 2) as usize] as char);
        encoded.push(TABLE[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(third & 0b111111) as usize] as char);
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::McpConnectionOperation;
    use super::{McpCallback, McpConnection, McpPageState, PendingMcpOperation, base64_url};
    use bevy::prelude::*;
    use bevy::tasks::{IoTaskPool, TaskPoolBuilder};
    use bevy_cef::prelude::UiInput;
    use vmux_api::mcp::McpServerRequest;
    use vmux_ecs::EntityTarget;
    use vmux_ecs::{Operation, OperationRetained, OperationTarget};

    #[test]
    fn pkce_uses_unpadded_url_safe_base64() {
        assert_eq!(base64_url(&[0xfb, 0xff]), "-_8");
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
        assert!(McpConnection::https_url("http://example.com", "endpoint").is_err());
        assert!(McpConnection::https_url("https://example.com", "endpoint").is_ok());
    }

    #[test]
    fn authorization_metadata_preserves_the_issuer_path() {
        assert_eq!(
            McpConnection::well_known_url(
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
    fn closing_a_page_retains_accepted_connection_io_until_completion() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        let page = app
            .world_mut()
            .spawn(McpPageState {
                generation: 1,
                ..Default::default()
            })
            .id();
        let server = app
            .world_mut()
            .spawn(super::McpCatalogServer {
                id: "linear".into(),
                url: "https://mcp.linear.app/mcp".into(),
                scopes: Vec::new(),
            })
            .id();
        let (release, wait) = std::sync::mpsc::channel();
        let (_, progress) = std::sync::mpsc::channel();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.connect").unwrap().running(),
                OperationRetained,
                OperationTarget::new(page),
                EntityTarget::<super::McpCatalogServer>::new(server),
                McpConnectionOperation,
                super::McpOperationTask {
                    generation: 1,
                    id: "linear".into(),
                    disconnect: false,
                    task: IoTaskPool::get().spawn(async move {
                        wait.recv().unwrap();
                        Ok(())
                    }),
                    progress: parking_lot::Mutex::new(progress),
                },
            ))
            .id();
        app.world_mut()
            .entity_mut(server)
            .insert(super::McpServerClaim {
                operation: Some(operation),
            });
        app.world_mut().despawn(page);
        for _ in 0..3 {
            app.update();
        }
        assert!(
            app.world()
                .get::<super::McpOperationTask>(operation)
                .is_some()
        );
        assert!(app.world().get::<OperationRetained>(operation).is_some());
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app
            .world()
            .get::<McpConnectionOperation>(operation)
            .is_some()
        {
            app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "accepted MCP IO did not drain"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.world().get::<OperationRetained>(operation).is_none());
        assert!(
            app.world()
                .get::<EntityTarget<McpConnectionOperation>>(server)
                .is_none()
        );
    }

    #[test]
    fn snapshot_from_an_old_profile_cannot_replace_current_global_observations() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        let old_owner = app.world_mut().spawn_empty().id();
        let current_owner = app.world_mut().spawn_empty().id();
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
