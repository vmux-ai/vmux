use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::{McpServerManifest, McpTransport, load_manifest, write_manifest};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use parking_lot::Mutex;
use reqwest::blocking::{Client, Response};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use url::Url;
use vmux_api::command_bar::{CommandBarQuery, CommandPaletteDraftRequest};
use vmux_api::mcp::{
    McpServerEntry, McpServerOperation, McpServerPending, McpServerRequest, McpServerResult,
    McpServerStatus, McpServers, McpServersRequest,
};
use vmux_core::host::{UiStatePlugin, UiStateWrite};
use vmux_core::profile::mcp_credentials::{
    McpCredentialAccess, McpCredentialStorage, McpOauthCredentials,
};

pub struct McpConnectionPlugin;

impl Plugin for McpConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(McpRuntime);
        app.add_plugins((
            UiEventPlugin::<(McpServersRequest, McpServerRequest)>::default(),
            UiStatePlugin::<McpServers>::default(),
        ))
        .add_message::<McpSnapshotRequest>()
        .add_observer(request_mcp_connections)
        .add_observer(request_palette_mcp_connections)
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
            )
                .chain(),
        );
    }
}

fn request_mcp_connections(trigger: On<UiInput<McpServersRequest>>, mut commands: Commands) {
    commands.trigger(RequestMcpSnapshot {
        target: trigger.event().webview,
    });
}

fn request_palette_mcp_connections(
    trigger: On<UiInput<CommandPaletteDraftRequest>>,
    active: Query<(), With<McpPaletteActive>>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let wants_mcp = CommandBarQuery(&trigger.event().payload.query)
        .mcp_filter()
        .is_some();
    match (wants_mcp, active.contains(target)) {
        (true, false) => {
            commands.entity(target).insert(McpPaletteActive);
            commands.trigger(RequestMcpSnapshot { target });
        }
        (false, true) => {
            commands.entity(target).remove::<McpPaletteActive>();
        }
        _ => {}
    }
}

fn begin_mcp_snapshot(
    trigger: On<RequestMcpSnapshot>,
    browsers: NonSend<Browsers>,
    mut states: Query<&mut McpPageState>,
    mut requests: MessageWriter<McpSnapshotRequest>,
    mut commands: Commands,
) {
    let target = trigger.event().target;
    if !browsers.can_emit_to(&target) {
        return;
    }
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
                snapshot: McpServers {
                    loading: true,
                    ..Default::default()
                },
            };
            let generation = state.generation;
            commands.entity(target).insert(state);
            generation
        }
    };
    requests.write(McpSnapshotRequest {
        target,
        generation,
        result: None,
    });
}

fn request_mcp_server(
    trigger: On<UiInput<McpServerRequest>>,
    runtime: Single<Entity, With<McpRuntime>>,
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
    commands.spawn((
        McpOperation { runtime: *runtime },
        PendingMcpOperation {
            target,
            generation,
            id,
            operation,
        },
    ));
}

fn start_mcp_operation(
    runtime: Query<&McpOperations, With<McpRuntime>>,
    pending: Query<&PendingMcpOperation>,
    running: Query<(), With<McpOperationTask>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok(runtime) = runtime.single() else {
        return;
    };
    let Some(entity) = runtime.iter().next() else {
        return;
    };
    if running.contains(entity) {
        return;
    }
    let Ok(pending) = pending.get(entity) else {
        return;
    };
    let target = pending.target;
    let generation = pending.generation;
    let id = pending.id.clone();
    let operation = pending.operation;
    let task_id = id.clone();
    let completion_wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let progress_wake = completion_wake.clone();
    let (progress_sender, progress_receiver) = mpsc::channel();
    let task = IoTaskPool::get().spawn(async move {
        let result = match operation {
            McpServerOperation::Connect => McpConnection::connect(&task_id, |url| {
                if progress_sender.send(url).is_ok()
                    && let Some(wake) = &progress_wake
                {
                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                }
            }),
            McpServerOperation::Disconnect => McpConnection::disconnect(&task_id),
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
            target,
            generation,
            id,
            operation,
            task,
            progress: Mutex::new(progress_receiver),
        });
}

fn drain_mcp_operations(
    mut tasks: Query<(Entity, &mut McpOperationTask)>,
    browsers: NonSend<Browsers>,
    mut stack_requests: MessageWriter<vmux_layout::stack::OpenRequest>,
    mut snapshot_requests: MessageWriter<McpSnapshotRequest>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        while let Ok(url) = task.progress.get_mut().try_recv() {
            if browsers.can_emit_to(&task.target) {
                stack_requests.write(vmux_layout::stack::OpenRequest { url: Some(url) });
            }
        }
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let (success, message) = match result {
            Ok(()) => (true, String::new()),
            Err(message) => (false, message),
        };
        if !browsers.can_emit_to(&task.target) {
            continue;
        }
        snapshot_requests.write(McpSnapshotRequest {
            target: task.target,
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
    mut requests: MessageReader<McpSnapshotRequest>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let target = request.target;
        let generation = request.generation;
        let result = request.result.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        commands.spawn(McpSnapshotTask {
            target,
            generation,
            task: IoTaskPool::get().spawn(async move {
                let snapshot = McpCatalog::snapshot(result);
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
        commands.entity(entity).despawn();
        let Ok(mut state) = states.get_mut(task.target) else {
            continue;
        };
        if task.generation == state.generation {
            state.snapshot = snapshot;
        }
    }
}

fn publish_mcp_connections(
    states: Query<(Entity, &McpPageState), Changed<McpPageState>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (target, state) in &states {
        if browsers.can_emit_to(&target) {
            commands.trigger(UiStateWrite::<McpServers>::from_event(
                target,
                &state.snapshot,
            ));
        }
    }
}

#[derive(Component)]
struct McpRuntime;

#[derive(Component)]
struct McpPaletteActive;

#[derive(EntityEvent)]
struct RequestMcpSnapshot {
    #[event_target]
    target: Entity,
}

#[derive(Component, Default)]
struct McpPageState {
    generation: u64,
    snapshot: McpServers,
}

#[derive(Component)]
#[relationship(relationship_target = McpOperations)]
struct McpOperation {
    #[relationship]
    runtime: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = McpOperation)]
struct McpOperations(Vec<Entity>);

#[derive(Component)]
struct PendingMcpOperation {
    target: Entity,
    generation: u64,
    id: String,
    operation: McpServerOperation,
}

#[derive(Clone, Message)]
struct McpSnapshotRequest {
    target: Entity,
    generation: u64,
    result: Option<McpServerResult>,
}

#[derive(Component)]
struct McpOperationTask {
    target: Entity,
    generation: u64,
    id: String,
    operation: McpServerOperation,
    task: Task<Result<(), String>>,
    progress: Mutex<mpsc::Receiver<String>>,
}

#[derive(Component)]
struct McpSnapshotTask {
    target: Entity,
    generation: u64,
    task: Task<McpServers>,
}

#[derive(Clone, Copy)]
struct McpCatalogEntry {
    id: &'static str,
    name: &'static str,
    url: &'static str,
    scopes: &'static [&'static str],
}

impl McpCatalogEntry {
    fn server(self) -> McpServerManifest {
        McpServerManifest {
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: Default::default(),
            cwd: None,
            url: Some(self.url.to_string()),
            headers: Default::default(),
            header_env: Default::default(),
            bearer_token_env_var: None,
        }
    }

    fn owns(self, server: &McpServerManifest) -> bool {
        server == &self.server()
    }
}

struct McpCatalog;

impl McpCatalog {
    const ENTRIES: [McpCatalogEntry; 1] = [McpCatalogEntry {
        id: "linear",
        name: "Linear",
        url: "https://mcp.linear.app/mcp",
        scopes: &["read", "write"],
    }];

    fn get(id: &str) -> Option<McpCatalogEntry> {
        Self::ENTRIES.iter().copied().find(|entry| entry.id == id)
    }

    fn snapshot(result: Option<McpServerResult>) -> McpServers {
        let manifest = load_manifest().unwrap_or_default();
        let mut servers = Vec::new();
        let mut catalog_ids = BTreeSet::new();
        for entry in Self::ENTRIES {
            catalog_ids.insert(entry.id);
            let configured = manifest
                .mcp
                .servers
                .get(entry.id)
                .is_some_and(|server| entry.owns(server));
            let occupied = manifest.mcp.servers.contains_key(entry.id);
            let authenticated = configured
                && McpCredentialStorage::load(entry.id)
                    .ok()
                    .flatten()
                    .is_some_and(|credentials| credentials.authorizes(entry.url));
            let status = match (configured, authenticated, occupied) {
                (false, _, true) => McpServerStatus::Configured,
                (true, true, _) => McpServerStatus::Connected,
                (true, false, _) => McpServerStatus::AuthenticationRequired,
                (false, _, false) => McpServerStatus::Available,
            };
            servers.push(McpServerEntry {
                id: entry.id.to_string(),
                name: entry.name.to_string(),
                description: String::new(),
                status,
            });
        }
        for (id, server) in manifest.mcp.servers {
            if catalog_ids.contains(id.as_str()) || id == "vmux" {
                continue;
            }
            let description = server
                .url
                .or(server.command)
                .unwrap_or_else(|| format!("{:?}", server.transport).to_ascii_lowercase());
            servers.push(McpServerEntry {
                name: id.clone(),
                id,
                description,
                status: McpServerStatus::Configured,
            });
        }
        McpServers {
            loaded: true,
            loading: false,
            servers,
            pending: None,
            result,
        }
    }
}

struct McpConnection;

impl McpConnection {
    fn connect(id: &str, progress: impl FnOnce(String)) -> Result<(), String> {
        let entry = McpCatalog::get(id).ok_or_else(|| format!("Unknown MCP server: {id}"))?;
        Self::ensure_catalog_slot(entry)?;
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
        let protected = Self::protected_metadata(&client, entry.url)?;
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
            .append_pair("resource", entry.url)
            .append_pair("scope", &entry.scopes.join(" "));
        progress(url.to_string());
        let code = McpCallback::receive(listener, &state)?;
        let token = Self::exchange(
            &client,
            &authorization.token_endpoint,
            &registration,
            &code,
            &verifier,
            &redirect_uri,
            entry.url,
        )?;
        let credentials = McpOauthCredentials {
            token_endpoint: authorization.token_endpoint,
            client_id: registration.client_id,
            client_secret: registration.client_secret,
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at: McpOauthCredentials::expires_at(token.expires_in),
            scope: token.scope.unwrap_or_else(|| entry.scopes.join(" ")),
            resource: entry.url.to_string(),
        };
        McpCredentialAccess::write(|| {
            Self::ensure_catalog_slot(entry)?;
            let original_credentials = McpCredentialStorage::load(id)?;
            McpCredentialStorage::store(id, &credentials)?;
            if let Err(error) = Self::write_manifest(entry) {
                let credentials_rollback = original_credentials
                    .as_ref()
                    .map(|credentials| McpCredentialStorage::store(id, credentials))
                    .unwrap_or_else(|| McpCredentialStorage::remove(id));
                return Err(Self::rollback_error(error, Ok(()), credentials_rollback));
            }
            Ok(())
        })
    }

    fn disconnect(id: &str) -> Result<(), String> {
        let entry = McpCatalog::get(id).ok_or_else(|| format!("Unknown MCP server: {id}"))?;
        McpCredentialAccess::write(|| {
            let credentials = McpCredentialStorage::load(id)?;
            let original = load_manifest()?;
            let server = original
                .mcp
                .servers
                .get(id)
                .ok_or_else(|| format!("MCP server is not configured: {id}"))?;
            if !entry.owns(server) {
                return Err(format!(
                    "MCP server ID is already managed by tools.toml: {id}"
                ));
            }
            let mut updated = original.clone();
            updated.mcp.servers.remove(id);
            write_manifest(&updated)?;
            if let Err(error) = McpCredentialStorage::remove(id) {
                let manifest_rollback = write_manifest(&original);
                let credentials_rollback = credentials
                    .as_ref()
                    .map(|credentials| McpCredentialStorage::store(id, credentials))
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

    fn write_manifest(entry: McpCatalogEntry) -> Result<(), String> {
        let mut manifest = load_manifest()?;
        if let Some(server) = manifest.mcp.servers.get(entry.id)
            && !entry.owns(server)
        {
            return Err(format!(
                "MCP server ID is already managed by tools.toml: {}",
                entry.id
            ));
        }
        manifest
            .mcp
            .servers
            .insert(entry.id.to_string(), entry.server());
        write_manifest(&manifest)
    }

    fn ensure_catalog_slot(entry: McpCatalogEntry) -> Result<(), String> {
        let manifest = load_manifest()?;
        let Some(server) = manifest.mcp.servers.get(entry.id) else {
            return Ok(());
        };
        if entry.owns(server) {
            return Ok(());
        }
        Err(format!(
            "MCP server ID is already managed by tools.toml: {}",
            entry.id
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
    use super::{
        McpCallback, McpConnection, McpOperation, McpOperations, McpPageState, McpRuntime,
        PendingMcpOperation, base64_url, request_mcp_server,
    };
    use bevy::prelude::*;
    use bevy_cef::prelude::UiInput;
    use vmux_api::mcp::{McpServerOperation, McpServerPending, McpServerRequest, McpServers};

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
    fn server_request_queues_one_operation_on_the_page_entity() {
        let mut app = App::new();
        app.add_observer(request_mcp_server);
        app.world_mut().spawn(McpRuntime);
        let target = app
            .world_mut()
            .spawn(McpPageState {
                generation: 1,
                snapshot: McpServers {
                    loaded: true,
                    servers: vec![vmux_api::mcp::McpServerEntry {
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
        let operations = app
            .world_mut()
            .query::<&PendingMcpOperation>()
            .iter(app.world())
            .count();
        assert_eq!(operations, 1);
    }

    #[test]
    fn mcp_actions_keep_entity_insertion_order() {
        let mut world = World::new();
        let runtime = world.spawn(McpRuntime).id();
        let first = world.spawn(McpOperation { runtime }).id();
        let second = world.spawn(McpOperation { runtime }).id();

        let operations = world.get::<McpOperations>(runtime).unwrap();
        assert_eq!(operations.0, [first, second]);
    }
}
