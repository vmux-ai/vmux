use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crate::{McpServerManifest, ToolStore};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use parking_lot::Mutex;
use reqwest::blocking::{Client, Response};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use url::Url;
use vmux_api::mcp::{
    McpAuthorization, McpLinkOpenRequest, McpServerCancelRequest, McpServerEntry,
    McpServerOperation, McpServerPending, McpServerRequest, McpServerResult, McpServerStatus,
    McpServersRequest, McpServersUiState,
};
use vmux_ecs::manifest::FeatureManifest;
use vmux_ecs::profile::McpCredentials;
use vmux_ecs::profile::mcp_credentials::McpOauthCredentials;
use vmux_ecs::{PageOpenRequest, PageOpenTarget};
use vmux_ecs::{UiStatePlugin, UiStateWrite};

pub struct McpConnectionPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConnectionSet;

#[derive(Message, Clone, Debug)]
pub enum ConnectorPopupRequest {
    Open {
        source: Entity,
        generation: u64,
        provider: String,
        name: String,
        url: String,
        code: Option<String>,
    },
    Close {
        source: Entity,
        generation: u64,
    },
}

impl Plugin for McpConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiEventPlugin::<(
                McpServersRequest,
                McpServerRequest,
                McpServerCancelRequest,
                McpLinkOpenRequest,
            )>::default(),
            UiStatePlugin::<McpServersUiState>::default(),
        ))
        .add_systems(Startup, spawn_mcp_catalog)
        .add_observer(cancel)
        .add_message::<PageOpenRequest>()
        .add_message::<ConnectorPopupRequest>()
        .add_observer(open_link)
        .add_observer(page_ready)
        .add_observer(request_mcp_connections)
        .add_observer(begin_mcp_snapshot)
        .add_observer(request_mcp_server)
        .add_systems(
            Update,
            (
                start_mcp_operation,
                drain_mcp_operations,
                start_mcp_snapshots,
                drain_mcp_snapshots,
                publish_mcp_connections,
                start_links,
                finish_links,
            )
                .chain()
                .in_set(ConnectionSet),
        );
    }
}

#[derive(Component)]
struct ExternalLink(String);

#[derive(Component)]
struct ExternalLinkTask(Task<Result<(), String>>);

fn open_link(
    trigger: On<UiInput<McpLinkOpenRequest>>,
    states: Query<&McpPageState>,
    catalog: McpCatalog,
    mut opens: MessageWriter<PageOpenRequest>,
    mut popups: MessageWriter<ConnectorPopupRequest>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let url = &event.payload.url;
    if McpConnection::https_url(url, "connector link").is_err() {
        return;
    }
    let sign_in = states.get(event.webview).is_ok_and(|state| {
        state
            .snapshot
            .authorization
            .as_ref()
            .is_some_and(|authorization| &authorization.url == url)
    });
    let setup = catalog
        .servers
        .iter()
        .any(|(_, server)| server.setup_url.as_ref() == Some(url));
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
        let Ok(state) = states.get(event.webview) else {
            return;
        };
        let Some(pending) = &state.snapshot.pending else {
            return;
        };
        let Some(name) = catalog.name(&pending.id) else {
            return;
        };
        popups.write(ConnectorPopupRequest::Open {
            source: event.webview,
            generation: state.generation,
            provider: pending.id.clone(),
            name,
            url: url.clone(),
            code: state
                .snapshot
                .authorization
                .as_ref()
                .and_then(|authorization| authorization.code.clone()),
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

#[derive(Component, Clone, Default)]
struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::Acquire) {
            return Err("Authentication cancelled".to_string());
        }
        Ok(())
    }

    fn wait(&self, duration: Duration) -> Result<(), String> {
        let deadline = Instant::now() + duration;
        loop {
            self.check()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            std::thread::sleep(remaining.min(Duration::from_millis(50)));
        }
    }
}

fn cancel(
    trigger: On<UiInput<McpServerCancelRequest>>,
    cancellations: Query<&Cancellation>,
    mut states: Query<&mut McpPageState>,
    mut popups: MessageWriter<ConnectorPopupRequest>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok(cancellation) = cancellations.get(target) else {
        return;
    };
    cancellation.cancel();
    commands
        .entity(target)
        .remove::<(McpOperationTask, PendingMcpOperation, Cancellation)>();
    if let Ok(mut state) = states.get_mut(target) {
        popups.write(ConnectorPopupRequest::Close {
            source: target,
            generation: state.generation,
        });
        state.snapshot.pending = None;
        state.snapshot.authorization = None;
        state.snapshot.result = None;
    }
    commands.trigger(McpSnapshotRequest { target });
}

fn page_ready(
    trigger: On<UiInput<vmux_api::PageReady>>,
    pages: Query<&vmux_ecs::PageMetadata>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    if pages.get(target).is_ok_and(|page| {
        vmux_api::RoutePattern::new(crate::CONNECTORS_URL, false).matches(&page.url)
    }) {
        commands.trigger(McpSnapshotRequest { target });
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
            if state.snapshot.loading || state.snapshot.pending.is_some() {
                return;
            }
            state.generation = state.generation.wrapping_add(1).max(1);
            state.snapshot.loading = true;
            state.snapshot.result = None;
            state.generation
        }
        Err(_) => {
            let state = McpPageState {
                generation: 1,
                snapshot: McpServersUiState {
                    loading: true,
                    ..Default::default()
                },
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
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let id = trigger.event().payload.id.clone();
    let Ok(mut state) = states.get_mut(target) else {
        return;
    };
    if state.snapshot.loading || state.snapshot.pending.is_some() {
        return;
    }
    let Some(server) = state.snapshot.servers.iter().find(|server| server.id == id) else {
        return;
    };
    let operation = match server.status {
        McpServerStatus::Available
        | McpServerStatus::AuthenticationRequired
        | McpServerStatus::Failed => McpServerOperation::Connect,
        McpServerStatus::Connected => McpServerOperation::Disconnect,
        McpServerStatus::Configured => return,
    };
    state.generation = state.generation.wrapping_add(1).max(1);
    let generation = state.generation;
    state.snapshot.pending = Some(McpServerPending {
        id: id.clone(),
        operation,
    });
    state.snapshot.result = None;
    commands.entity(target).insert(PendingMcpOperation {
        generation,
        id,
        operation,
    });
}

fn start_mcp_operation(
    pending: Query<(Entity, &PendingMcpOperation), Without<McpOperationTask>>,
    catalog: McpCatalog,
    credentials: Query<&McpCredentials>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok(credentials) = credentials.single() else {
        return;
    };
    for (entity, pending) in &pending {
        let generation = pending.generation;
        let id = pending.id.clone();
        let operation = pending.operation;
        let server = catalog.server(&id);
        let credentials = (*credentials).clone();
        let task_id = id.clone();
        let completion_wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let progress_wake = completion_wake.clone();
        let (progress_sender, progress_receiver) = mpsc::channel();
        let cancellation = Cancellation::default();
        let operation_cancellation = cancellation.clone();
        let task = IoTaskPool::get().spawn(async move {
            let result = match (operation, server.as_ref()) {
                (McpServerOperation::Connect, Some(server)) => {
                    McpConnection::connect(server, &credentials, &operation_cancellation, |url| {
                        if progress_sender.send(url).is_ok()
                            && let Some(wake) = &progress_wake
                        {
                            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                        }
                    })
                    .await
                }
                (McpServerOperation::Disconnect, Some(server)) => {
                    McpConnection::disconnect(server, &credentials).await
                }
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
            .insert(cancellation.clone())
            .insert(McpOperationTask {
                generation,
                id,
                operation,
                cancellation,
                task,
                progress: Mutex::new(progress_receiver),
            });
    }
}

fn drain_mcp_operations(
    mut tasks: Query<(Entity, &mut McpOperationTask)>,
    mut states: Query<&mut McpPageState>,
    catalog: McpCatalog,
    mut popups: MessageWriter<ConnectorPopupRequest>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        while let Ok(authorization) = task.progress.get_mut().try_recv() {
            if let Ok(mut state) = states.get_mut(entity)
                && state.generation == task.generation
            {
                if let Some(name) = catalog.name(&task.id) {
                    popups.write(ConnectorPopupRequest::Open {
                        source: entity,
                        generation: task.generation,
                        provider: task.id.clone(),
                        name,
                        url: authorization.url.clone(),
                        code: authorization.code.clone(),
                    });
                }
                state.snapshot.authorization = Some(authorization);
            }
        }
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        popups.write(ConnectorPopupRequest::Close {
            source: entity,
            generation: task.generation,
        });
        if let Ok(mut state) = states.get_mut(entity)
            && state.generation == task.generation
        {
            state.snapshot.authorization = None;
        }
        commands
            .entity(entity)
            .remove::<(McpOperationTask, Cancellation)>();
        let (success, message) = match result {
            Ok(()) => (true, String::new()),
            Err(message) => (false, message),
        };
        commands.entity(entity).insert(PendingMcpSnapshot {
            generation: task.generation,
            result: Some(McpServerResult {
                id: task.id.clone(),
                operation: task.operation,
                success,
                message,
            }),
        });
    }
}

fn start_mcp_snapshots(
    requests: Query<(Entity, &PendingMcpSnapshot), Without<McpSnapshotTask>>,
    catalog: McpCatalog,
    credentials: Query<&McpCredentials>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok(credentials) = credentials.single() else {
        return;
    };
    for (target, request) in &requests {
        let generation = request.generation;
        let result = request.result.clone();
        let source = catalog.snapshot_source();
        let credentials = (*credentials).clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        commands
            .entity(target)
            .remove::<PendingMcpSnapshot>()
            .insert(McpSnapshotTask {
                generation,
                task: IoTaskPool::get().spawn(async move {
                    let snapshot = source.load(result, &credentials);
                    if let Some(wake) = wake {
                        let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                    }
                    snapshot
                }),
            });
    }
}

fn drain_mcp_snapshots(
    mut tasks: Query<(Entity, &mut McpSnapshotTask)>,
    mut states: Query<&mut McpPageState>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(snapshot) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).remove::<McpSnapshotTask>();
        let Ok(mut state) = states.get_mut(entity) else {
            continue;
        };
        if task.generation == state.generation {
            state.snapshot = snapshot;
        }
    }
}

fn publish_mcp_connections(
    states: Query<(Entity, &McpPageState), Changed<McpPageState>>,
    mut commands: Commands,
) {
    for (target, state) in &states {
        commands.trigger(UiStateWrite::<McpServersUiState>::from_event(
            target,
            &state.snapshot,
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
    snapshot: McpServersUiState,
}

#[derive(Component)]
struct PendingMcpOperation {
    generation: u64,
    id: String,
    operation: McpServerOperation,
}

#[derive(Component, Clone)]
struct PendingMcpSnapshot {
    generation: u64,
    result: Option<McpServerResult>,
}

#[derive(Component)]
struct McpOperationTask {
    generation: u64,
    id: String,
    operation: McpServerOperation,
    cancellation: Cancellation,
    task: Task<Result<(), String>>,
    progress: Mutex<mpsc::Receiver<McpAuthorization>>,
}

impl Drop for McpOperationTask {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

#[derive(Component)]
struct McpSnapshotTask {
    generation: u64,
    task: Task<McpServersUiState>,
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
struct McpCatalog<'w, 's> {
    servers: Query<'w, 's, (&'static Name, &'static McpCatalogServer)>,
}

impl McpCatalog<'_, '_> {
    fn name(&self, id: &str) -> Option<String> {
        self.servers
            .iter()
            .find(|(_, server)| server.id == id)
            .map(|(name, _)| name.as_str().to_string())
    }

    fn server(&self, id: &str) -> Option<McpCatalogServer> {
        self.servers
            .iter()
            .find_map(|(_, server)| (server.id == id).then(|| server.clone()))
    }

    fn snapshot_source(&self) -> McpSnapshotSource {
        McpSnapshotSource(
            self.servers
                .iter()
                .map(|(name, server)| (name.as_str().to_string(), server.clone()))
                .collect(),
        )
    }
}

struct McpSnapshotSource(Vec<(String, McpCatalogServer)>);

impl McpSnapshotSource {
    fn load(
        self,
        result: Option<McpServerResult>,
        credentials: &McpCredentials,
    ) -> McpServersUiState {
        let manifest = ToolStore::current().load().unwrap_or_default();
        let mut servers = Vec::new();
        for (name, server) in self.0 {
            let configured = manifest
                .mcp
                .servers
                .get(&server.id)
                .is_some_and(|manifest| server.owns(manifest));
            let occupied = manifest.mcp.servers.contains_key(&server.id);
            let stored = credentials.storage.load(&server.id).ok().flatten();
            let authenticated = configured
                && stored.as_ref().is_some_and(|credentials| {
                    credentials.authorizes(&server.url)
                        && (!credentials.expires_soon()
                            || credentials
                                .refresh_token
                                .as_ref()
                                .is_some_and(|token| !token.is_empty()))
                });
            let status = match (configured, authenticated, occupied) {
                (false, _, true) => McpServerStatus::Configured,
                (true, true, _) => McpServerStatus::Connected,
                (true, false, _) => McpServerStatus::AuthenticationRequired,
                (false, _, false) => McpServerStatus::Available,
            };
            let account = stored.and_then(|credentials| credentials.account);
            servers.push(McpServerEntry {
                setup_url: server.setup_url,
                account,
                id: server.id,
                name,
                description: String::new(),
                status,
            });
        }
        McpServersUiState {
            selected: Vec::new(),
            loaded: true,
            loading: false,
            servers,
            pending: None,
            result,
            authorization: None,
        }
    }
}

struct McpConnection;

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

impl McpConnection {
    async fn connect(
        server: &McpCatalogServer,
        credentials: &McpCredentials,
        cancellation: &Cancellation,
        progress: impl FnMut(McpAuthorization),
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
            .user_agent("Vmux")
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?;
        let protected = Self::protected_metadata(&client, &server.url)?;
        let issuer = protected
            .authorization_servers
            .first()
            .ok_or_else(|| "MCP server did not advertise an authorization server".to_string())?;
        let authorization = Self::authorization_metadata(&client, issuer)?;
        let (registration, token) = if authorization.registration_endpoint.is_some() {
            Self::authorization_code(
                &client,
                &authorization,
                &redirect_uri,
                listener,
                server,
                cancellation,
                progress,
            )?
        } else {
            DeviceFlow::authorize(&client, &authorization, server, cancellation, progress)?
        };
        let account = match &server.account_endpoint {
            Some(endpoint) => {
                let response: Result<serde_json::Value, String> =
                    Self::https_url(endpoint, "account endpoint").and_then(|endpoint| {
                        Self::json(client.get(endpoint).bearer_auth(&token.access_token).send())
                    });
                match response {
                    Ok(value) => value
                        .get("preferred_username")
                        .or_else(|| value.get("login"))
                        .or_else(|| value.get("name"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    Err(error) => {
                        warn!("Connector account lookup failed: {error}");
                        None
                    }
                }
            }
            None => None,
        };
        let oauth = McpOauthCredentials {
            account,
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
            cancellation.check()?;
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

    fn authorization_code(
        client: &Client,
        authorization: &AuthorizationServer,
        redirect_uri: &str,
        listener: TcpListener,
        server: &McpCatalogServer,
        cancellation: &Cancellation,
        mut progress: impl FnMut(McpAuthorization),
    ) -> Result<(ClientRegistration, TokenResponse), String> {
        let registration = Self::register(client, authorization, redirect_uri)?;
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
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("resource", &server.url)
            .append_pair("scope", &server.scopes.join(" "));
        progress(McpAuthorization {
            url: url.to_string(),
            code: None,
        });
        let code = McpCallback::receive(listener, &state, cancellation)?;
        let token = Self::exchange(
            client,
            &authorization.token_endpoint,
            &registration,
            &code,
            &verifier,
            redirect_uri,
            &server.url,
        )?;
        Ok((registration, token))
    }

    async fn disconnect(
        server: &McpCatalogServer,
        credentials: &McpCredentials,
    ) -> Result<(), String> {
        let store = ToolStore::current();
        let _lease = store.lease_async().await;
        credentials
            .access
            .write(|| credentials.storage.remove(&server.id))
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
    fn receive(
        listener: TcpListener,
        expected_state: &str,
        cancellation: &Cancellation,
    ) -> Result<String, String> {
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            cancellation.check()?;
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
    #[serde(default)]
    device_authorization_endpoint: Option<String>,
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

struct DeviceFlow;

impl DeviceFlow {
    fn authorize(
        client: &Client,
        authorization: &AuthorizationServer,
        server: &McpCatalogServer,
        cancellation: &Cancellation,
        mut progress: impl FnMut(McpAuthorization),
    ) -> Result<(ClientRegistration, TokenResponse), String> {
        let client_id = server
            .client_id_env
            .as_deref()
            .and_then(|key| std::env::var(key).ok())
            .or_else(|| server.client_id.clone())
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| {
                "This connector has no registered OAuth client ID in this build".to_string()
            })?;
        let endpoint = authorization
            .device_authorization_endpoint
            .as_deref()
            .ok_or_else(|| "Authorization server does not support device flow".to_string())?;
        let endpoint = McpConnection::https_url(endpoint, "device authorization endpoint")?;
        let response: DeviceAuthorization = McpConnection::json(
            client
                .post(endpoint)
                .header("Accept", "application/json")
                .form(&[
                    ("client_id", client_id.as_str()),
                    ("scope", server.scopes.join(" ").as_str()),
                ])
                .send(),
        )?;
        let url =
            McpConnection::https_url(&response.verification_uri, "verification URI")?.to_string();
        progress(McpAuthorization {
            url,
            code: Some(response.user_code),
        });
        let deadline = Instant::now() + Duration::from_secs(response.expires_in.min(900));
        let mut interval = response.interval.unwrap_or(5).max(1);
        let endpoint = McpConnection::https_url(&authorization.token_endpoint, "token endpoint")?;
        loop {
            if Instant::now() >= deadline {
                return Err("Authentication timed out".to_string());
            }
            cancellation.wait(
                Duration::from_secs(interval)
                    .min(deadline.saturating_duration_since(Instant::now())),
            )?;
            if Instant::now() >= deadline {
                return Err("Authentication timed out".to_string());
            }
            let response = client
                .post(endpoint.clone())
                .header("Accept", "application/json")
                .form(&[
                    ("client_id", client_id.as_str()),
                    ("device_code", response.device_code.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .map_err(|error| error.to_string())?;
            let result: DeviceToken = response.json().map_err(|error| error.to_string())?;
            if let Some(token) = result.poll(&mut interval)? {
                return Ok((
                    ClientRegistration {
                        client_id,
                        client_secret: None,
                    },
                    token,
                ));
            }
        }
    }
}

#[derive(Deserialize)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct DeviceToken {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    error: Option<String>,
}

impl DeviceToken {
    fn poll(self, interval: &mut u64) -> Result<Option<TokenResponse>, String> {
        if let Some(access_token) = self.access_token {
            if access_token.is_empty() {
                return Err("Authentication returned an empty token".to_string());
            }
            return Ok(Some(TokenResponse {
                access_token,
                refresh_token: self.refresh_token,
                expires_in: self.expires_in,
                scope: self.scope,
            }));
        }
        match self.error.as_deref() {
            Some("authorization_pending") => Ok(None),
            Some("slow_down") => {
                *interval = interval.saturating_add(5);
                Ok(None)
            }
            Some("access_denied") => Err("Authentication was denied".to_string()),
            Some("expired_token") => Err("Authentication timed out".to_string()),
            _ => Err("Authentication failed".to_string()),
        }
    }
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
    use super::{
        Cancellation, McpCallback, McpConnection, McpPageState, PendingMcpOperation, base64_url,
        request_mcp_server,
    };
    use bevy::prelude::*;
    use bevy_cef::prelude::UiInput;
    use vmux_api::mcp::{
        McpServerOperation, McpServerPending, McpServerRequest, McpServersUiState,
    };
    use vmux_ecs::manifest::FeatureManifest;

    #[test]
    fn connector_links_open_in_app_and_queue_external_opening_only_on_request() {
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        app.world_mut()
            .spawn(FeatureManifest::of::<crate::Feature>());
        app.update();
        let url = "https://github.com/login/device";
        let target = app
            .world_mut()
            .spawn(McpPageState {
                generation: 1,
                snapshot: McpServersUiState {
                    pending: Some(McpServerPending {
                        id: "github".into(),
                        operation: McpServerOperation::Connect,
                    }),
                    authorization: Some(vmux_api::mcp::McpAuthorization {
                        url: url.into(),
                        code: Some("ABCD-1234".into()),
                    }),
                    ..Default::default()
                },
            })
            .id();
        app.world_mut().trigger(UiInput {
            webview: target,
            payload: vmux_api::mcp::McpLinkOpenRequest {
                url: url.into(),
                external: false,
            },
        });
        app.world_mut().flush();
        let opens = app
            .world_mut()
            .resource_mut::<Messages<super::ConnectorPopupRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opens.len(), 1);
        assert!(
            matches!(&opens[0], super::ConnectorPopupRequest::Open { source, url: opened, .. } if *source == target && opened == url)
        );
        app.world_mut().trigger(UiInput {
            webview: target,
            payload: vmux_api::mcp::McpLinkOpenRequest {
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
                payload: vmux_api::mcp::McpLinkOpenRequest {
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
    fn sign_in_progress_requests_an_authentication_popup_and_keeps_the_verification_code() {
        let pool = bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        app.world_mut()
            .spawn(FeatureManifest::of::<crate::Feature>());
        let (send, receive) = std::sync::mpsc::channel();
        let authorization = vmux_api::mcp::McpAuthorization {
            url: "https://github.com/login/device".into(),
            code: Some("ABCD-1234".into()),
        };
        send.send(authorization.clone()).unwrap();
        let target = app
            .world_mut()
            .spawn((
                McpPageState {
                    generation: 1,
                    ..Default::default()
                },
                super::McpOperationTask {
                    generation: 1,
                    id: "github".into(),
                    operation: McpServerOperation::Connect,
                    cancellation: Cancellation::default(),
                    task: pool.spawn(std::future::pending()),
                    progress: super::Mutex::new(receive),
                },
            ))
            .id();
        app.update();
        let opens = app
            .world_mut()
            .resource_mut::<Messages<super::ConnectorPopupRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opens.len(), 1);
        assert!(
            matches!(&opens[0], super::ConnectorPopupRequest::Open { source, url, code, .. } if *source == target && *url == authorization.url && *code == authorization.code)
        );
        assert_eq!(
            app.world()
                .get::<McpPageState>(target)
                .unwrap()
                .snapshot
                .authorization,
            Some(authorization)
        );
    }

    #[test]
    fn disposing_an_authentication_operation_cancels_its_polling() {
        let pool = bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let cancellation = Cancellation::default();
        let (_, receive) = std::sync::mpsc::channel();
        let mut app = App::new();
        let entity = app
            .world_mut()
            .spawn(super::McpOperationTask {
                generation: 1,
                id: "github".into(),
                operation: McpServerOperation::Connect,
                cancellation: cancellation.clone(),
                task: pool.spawn(std::future::pending()),
                progress: super::Mutex::new(receive),
            })
            .id();
        app.world_mut().despawn(entity);
        assert!(cancellation.check().is_err());
    }

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
    fn device_authorization_waits_slows_down_and_stops_on_denial_or_expiry() {
        let mut interval = 5;
        for error in ["authorization_pending", "slow_down"] {
            let response: super::DeviceToken =
                serde_json::from_value(serde_json::json!({"error": error})).unwrap();
            assert!(response.poll(&mut interval).unwrap().is_none());
        }
        assert_eq!(interval, 10);
        for (error, message) in [
            ("access_denied", "Authentication was denied"),
            ("expired_token", "Authentication timed out"),
        ] {
            let response: super::DeviceToken =
                serde_json::from_value(serde_json::json!({"error": error})).unwrap();
            assert_eq!(response.poll(&mut interval).err().as_deref(), Some(message));
        }
        let response: super::DeviceToken = serde_json::from_value(
            serde_json::json!({"access_token": "approved", "expires_in": 28800}),
        )
        .unwrap();
        let token = response.poll(&mut interval).unwrap().unwrap();
        assert_eq!(token.access_token, "approved");
        assert_eq!(token.expires_in, Some(28800));
    }

    #[test]
    fn cancelling_device_sign_in_interrupts_the_polling_wait() {
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let started = std::time::Instant::now();
        assert!(
            cancellation
                .wait(std::time::Duration::from_secs(30))
                .is_err()
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn snapshot_requests_are_retained_before_the_browser_is_ready() {
        let mut app = App::new();
        app.add_plugins(super::McpConnectionPlugin);
        let target = app.world_mut().spawn_empty().id();
        app.world_mut()
            .trigger(super::McpSnapshotRequest { target });
        app.update();
        assert!(
            app.world()
                .get::<McpPageState>(target)
                .unwrap()
                .snapshot
                .loading
        );
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
    fn server_request_queues_one_operation_on_the_page_entity() {
        let mut app = App::new();
        app.add_observer(request_mcp_server);
        let target = app
            .world_mut()
            .spawn(McpPageState {
                generation: 1,
                snapshot: McpServersUiState {
                    loaded: true,
                    servers: vec![vmux_api::mcp::McpServerEntry {
                        setup_url: None,
                        account: None,
                        id: "linear".to_string(),
                        name: "Linear".to_string(),
                        description: String::new(),
                        status: vmux_api::mcp::McpServerStatus::Available,
                    }],
                    ..Default::default()
                },
            })
            .id();

        app.world_mut().trigger(UiInput {
            webview: target,
            payload: McpServerRequest {
                id: "linear".to_string(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview: target,
            payload: McpServerRequest {
                id: "linear".to_string(),
            },
        });
        app.world_mut().flush();

        let state = app.world().get::<McpPageState>(target).unwrap();
        assert_eq!(state.generation, 2);
        assert_eq!(
            state.snapshot.pending,
            Some(McpServerPending {
                id: "linear".to_string(),
                operation: McpServerOperation::Connect,
            })
        );
        let operation = app.world().get::<PendingMcpOperation>(target).unwrap();
        assert_eq!(operation.generation, 2);
        assert_eq!(operation.id, "linear");
        assert_eq!(operation.operation, McpServerOperation::Connect);
    }
}
