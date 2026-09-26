use bevy_app::{App, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use serde_json::{Value, json};
use std::future::Future;
use std::io::{self, BufRead, Write};
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;
use vmux_core::{HostShell, JsonArguments, ProcessAnchor};
use vmux_service::protocol::{
    AgentCommand, AgentQuery, AgentQueryResult, AgentRequestId, ClientMessage, ServiceMessage,
};

pub struct McpPlugin {
    config: McpConfig,
}

impl McpPlugin {
    pub fn new(
        anchor: Option<vmux_service::protocol::ProcessId>,
        acp_session: bool,
        acp_terminals: bool,
        run_block_timeout: Duration,
        shell: String,
    ) -> Self {
        Self {
            config: McpConfig {
                anchor,
                acp_session,
                acp_terminals,
                run_block_timeout,
                shell,
            },
        }
    }
}

impl Plugin for McpPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::tool::ToolRuntimePlugin>() {
            app.add_plugins(crate::tool::ToolRuntimePlugin);
        }
        app.world_mut().spawn((
            Name::new("MCP protocol runtime"),
            self.config.clone(),
            NextRequestSequence::default(),
        ));
        app.configure_sets(
            Update,
            (
                McpSet::Route,
                crate::tool::ToolResolveSet,
                crate::tool::ToolRequestSet,
                crate::tool::ToolDispatchSet,
                crate::tool::ToolDispatchFlush,
                McpSet::StartTasks,
                McpSet::BuildResponses,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                route_request.in_set(McpSet::Route),
                (
                    finish_tool_errors,
                    start_list_tools,
                    start_tool_commands,
                    start_tool_queries,
                    bevy_ecs::schedule::ApplyDeferred,
                    start_mcp_tasks,
                    bevy_ecs::schedule::ApplyDeferred,
                    poll_tool_tasks,
                )
                    .chain()
                    .in_set(McpSet::StartTasks),
                build_responses.in_set(McpSet::BuildResponses),
            ),
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
enum McpSet {
    Route,
    StartTasks,
    BuildResponses,
}

pub struct McpServer {
    app: App,
    runtime: Entity,
}

impl From<App> for McpServer {
    fn from(mut app: App) -> Self {
        let runtime = app
            .world_mut()
            .query_filtered::<Entity, With<McpConfig>>()
            .single(app.world())
            .expect("MCP server requires exactly one protocol runtime");
        Self { app, runtime }
    }
}

impl McpServer {
    pub fn new(
        anchor: Option<vmux_service::protocol::ProcessId>,
        acp_session: bool,
        acp_terminals: bool,
        run_block_timeout: Duration,
        shell: String,
    ) -> Self {
        let mut app = App::new();
        app.add_plugins(McpPlugin::new(
            anchor,
            acp_session,
            acp_terminals,
            run_block_timeout,
            shell,
        ));
        Self::from(app)
    }

    pub async fn handle(&mut self, message: Value) -> Option<Value> {
        self.app
            .world_mut()
            .entity_mut(self.runtime)
            .insert(McpRuntime(tokio::runtime::Handle::current()));
        let id = message.get("id").cloned()?;
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        let run_block_timeout = self
            .app
            .world()
            .get::<McpConfig>(self.runtime)
            .expect("MCP protocol runtime must own its config")
            .run_block_timeout;
        let sequence = {
            let mut runtime = self.app.world_mut().entity_mut(self.runtime);
            let mut next = runtime
                .get_mut::<NextRequestSequence>()
                .expect("MCP protocol runtime must own its request sequence");
            let sequence = next.0;
            next.0 += 1;
            sequence
        };
        let request = self
            .app
            .world_mut()
            .spawn((
                McpRequest::new(run_block_timeout),
                Name::new(format!("MCP request {sequence}")),
                McpRequestId(id.clone()),
                McpMethod(method),
                McpParams(params),
                McpRequestSequence(sequence),
            ))
            .id();

        loop {
            self.app.update();
            if let Some(response) = self
                .app
                .world_mut()
                .entity_mut(request)
                .take::<McpResponse>()
            {
                self.app.world_mut().despawn(request);
                return Some(response.0);
            }
            if !self.app.world().entity(request).contains::<McpTask>() {
                self.app.world_mut().despawn(request);
                return Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32603,
                        "message": "request did not produce a response"
                    }
                }));
            }
            tokio::task::yield_now().await;
        }
    }
}

#[derive(Component, Clone)]
struct McpConfig {
    anchor: Option<vmux_service::protocol::ProcessId>,
    acp_session: bool,
    acp_terminals: bool,
    run_block_timeout: Duration,
    shell: String,
}

#[derive(Component, Clone)]
struct McpRuntime(tokio::runtime::Handle);

#[derive(Component, Default)]
struct NextRequestSequence(u64);

#[derive(Component)]
pub struct McpRequest {
    run_block_timeout: Duration,
}

impl McpRequest {
    pub fn new(run_block_timeout: Duration) -> Self {
        Self { run_block_timeout }
    }

    pub fn run_block_timeout(&self) -> Duration {
        self.run_block_timeout
    }
}

#[derive(Component)]
struct McpRequestId(Value);

#[derive(Component)]
struct McpMethod(String);

#[derive(Component)]
struct McpParams(Value);

#[derive(Component)]
struct McpRequestSequence(u64);

#[derive(Component)]
struct McpRouted;

#[derive(Component)]
struct McpTask(tokio::sync::oneshot::Receiver<Result<Value, String>>);

type McpFuture = Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;

#[derive(Component)]
pub struct McpExecution(Mutex<Option<McpFuture>>);

impl McpExecution {
    pub fn new(future: impl Future<Output = Result<Value, String>> + Send + 'static) -> Self {
        Self(Mutex::new(Some(Box::pin(future))))
    }
}

#[derive(Component)]
enum McpReply {
    Result(Result<Value, String>),
    ProtocolError { code: i64, message: String },
}

#[derive(Component)]
struct McpResponse(Value);

type PendingRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static McpMethod,
        &'static McpParams,
        &'static McpRequestSequence,
    ),
    (With<McpRequest>, Without<McpRouted>),
>;

pub fn read_json_line(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 {
        return Ok(None);
    }
    let value = serde_json::from_str(line.trim_end())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(Some(value))
}

pub async fn run_stdio(mut server: McpServer) -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    while let Some(message) = read_json_line(&mut reader)? {
        if let Some(response) = server.handle(message).await {
            serde_json::to_writer(&mut writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn route_request(mut commands: Commands, requests: PendingRequests, config: Single<&McpConfig>) {
    let Some((entity, method, params, _)) =
        requests.iter().min_by_key(|(_, _, _, sequence)| sequence.0)
    else {
        return;
    };
    commands.entity(entity).insert(McpRouted);

    match method.0.as_str() {
        "initialize" => {
            commands
                .entity(entity)
                .insert(McpReply::Result(Ok(initialize_result(&params.0))));
        }
        "tools/list" => {
            let mut request = commands.entity(entity);
            request.insert((
                crate::tool::ToolCatalogRequest,
                HostShell(config.shell.clone()),
            ));
            if config.acp_session {
                request.insert(crate::tool::AcpSessionContext);
            }
            if config.acp_terminals {
                request.insert(crate::tool::AcpTerminalContext);
            }
        }
        "tools/call" => {
            let Some(name) = params.0.get("name").and_then(Value::as_str) else {
                commands
                    .entity(entity)
                    .insert(McpReply::Result(Err("tools/call missing name".to_string())));
                return;
            };
            let arguments = params
                .0
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let mut request = commands.entity(entity);
            request.insert((
                Name::new(name.to_string()),
                JsonArguments(arguments),
                HostShell(config.shell.clone()),
                crate::tool::ToolInvocation,
                crate::tool::ToolCommandFallback,
            ));
            if let Some(anchor) = config.anchor {
                request.insert(ProcessAnchor(anchor));
            }
            if config.acp_session {
                request.insert(crate::tool::AcpSessionContext);
            }
            if config.acp_terminals {
                request.insert(crate::tool::AcpTerminalContext);
            }
        }
        method => {
            commands.entity(entity).insert(McpReply::ProtocolError {
                code: -32601,
                message: format!("method not found: {method}"),
            });
        }
    }
}

fn finish_tool_errors(
    mut commands: Commands,
    errors: Query<(Entity, &crate::tool::ToolDispatchError), Added<crate::tool::ToolDispatchError>>,
) {
    for (entity, error) in &errors {
        commands
            .entity(entity)
            .remove::<crate::tool::ToolInvocation>()
            .remove::<crate::tool::ToolCall>()
            .remove::<crate::tool::ToolDispatchError>()
            .insert(McpReply::Result(Err(error.message().to_string())));
    }
}

fn start_list_tools(
    mut commands: Commands,
    requests: Query<(Entity, &crate::tool::ToolCatalog), Added<crate::tool::ToolCatalog>>,
) {
    for (entity, request) in &requests {
        let mut definitions = request.0.clone();
        commands
            .entity(entity)
            .remove::<crate::tool::ToolCatalogRequest>()
            .remove::<crate::tool::ToolCatalog>()
            .insert(McpExecution::new(async move {
                if let Ok(connection) = vmux_service::client::ServiceConnection::connect().await
                    && let Ok(AgentQueryResult::Commands(commands)) =
                        agent_query(&connection, AgentQuery::ListCommands).await
                {
                    definitions =
                        crate::tool::ToolDefinition::merge_commands(definitions, commands)?;
                }
                Ok(json!({ "tools": definitions }))
            }));
    }
}

fn start_tool_commands(
    mut commands: Commands,
    requests: Query<
        (Entity, Option<&ProcessAnchor>, &crate::tool::ToolCommand),
        Added<crate::tool::ToolCommand>,
    >,
) {
    for (entity, anchor, result) in &requests {
        let mut request = commands.entity(entity);
        request
            .remove::<crate::tool::ToolInvocation>()
            .remove::<crate::tool::ToolCall>()
            .remove::<crate::tool::ToolCommand>();
        let command = match result.0.clone() {
            Ok(command) => command,
            Err(message) => {
                request.insert(McpReply::Result(Err(message)));
                continue;
            }
        };
        request.insert(McpExecution::new(run_agent_command(
            command,
            anchor.map(|anchor| anchor.0),
        )));
    }
}

fn start_tool_queries(
    mut commands: Commands,
    requests: Query<(Entity, &crate::tool::ToolQuery), Added<crate::tool::ToolQuery>>,
) {
    for (entity, result) in &requests {
        let mut request = commands.entity(entity);
        request
            .remove::<crate::tool::ToolInvocation>()
            .remove::<crate::tool::ToolCall>()
            .remove::<crate::tool::ToolQuery>();
        let query = match result.0.clone() {
            Ok(query) => query,
            Err(message) => {
                request.insert(McpReply::Result(Err(message)));
                continue;
            }
        };
        request.insert(McpExecution::new(run_agent_query(query)));
    }
}

fn start_mcp_tasks(
    runtime: Single<&McpRuntime>,
    mut pending: Query<(Entity, &mut McpExecution), Added<McpExecution>>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut pending {
        let future = pending
            .0
            .get_mut()
            .unwrap()
            .take()
            .expect("pending MCP task must own its future");
        let (sender, receiver) = tokio::sync::oneshot::channel();
        drop(runtime.0.spawn(async move {
            let _ = sender.send(future.await);
        }));
        commands
            .entity(entity)
            .remove::<McpExecution>()
            .insert(McpTask(receiver));
    }
}

fn poll_tool_tasks(mut commands: Commands, mut tasks: Query<(Entity, &mut McpTask)>) {
    for (entity, mut task) in &mut tasks {
        let result = match task.0.try_recv() {
            Ok(result) => result,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => continue,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                Err("MCP tool task stopped before producing a response".to_string())
            }
        };
        commands
            .entity(entity)
            .remove::<McpTask>()
            .insert(McpReply::Result(result));
    }
}

fn build_responses(
    mut commands: Commands,
    replies: Query<(Entity, &McpRequestId, &McpReply), Without<McpResponse>>,
) {
    for (entity, id, reply) in &replies {
        let response = match reply {
            McpReply::Result(Ok(result)) => json!({
                "jsonrpc": "2.0",
                "id": id.0,
                "result": result
            }),
            McpReply::Result(Err(message)) => json!({
                "jsonrpc": "2.0",
                "id": id.0,
                "result": tool_error(message)
            }),
            McpReply::ProtocolError { code, message } => json!({
                "jsonrpc": "2.0",
                "id": id.0,
                "error": {
                    "code": code,
                    "message": message
                }
            }),
        };
        commands.entity(entity).insert(McpResponse(response));
    }
}

fn initialize_result(params: &Value) -> Value {
    let protocol_version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("2025-11-25");
    json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "vmux",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

async fn run_agent_command(
    command: AgentCommand,
    anchor: Option<vmux_service::protocol::ProcessId>,
) -> Result<Value, String> {
    let request_id = vmux_service::protocol::AgentRequestId::new();
    let connection = vmux_service::client::ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    connection
        .send(&ClientMessage::AgentCommand {
            request_id,
            anchor,
            command,
        })
        .await
        .map_err(|error| format!("cannot send agent command: {error}"))?;

    loop {
        let Some(message) = connection
            .recv()
            .await
            .map_err(|error| format!("cannot read service response: {error}"))?
        else {
            return Err("vmux_service disconnected".to_string());
        };
        match message {
            ServiceMessage::AgentCommandResult {
                request_id: received,
                result,
            } if received == request_id => {
                return command_result_to_mcp_response(result);
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}

pub fn command_result_to_mcp_response(
    result: vmux_service::protocol::AgentCommandResult,
) -> Result<Value, String> {
    use vmux_service::protocol::AgentCommandResult;
    match result {
        AgentCommandResult::Ok => Ok(json!({
            "content": [{"type": "text", "text": "ok"}]
        })),
        AgentCommandResult::Text(text) => Ok(json!({
            "content": [{"type": "text", "text": text}]
        })),
        AgentCommandResult::Layout(snapshot) => {
            let text = serde_json::to_string(&snapshot).unwrap_or_default();
            Ok(json!({
                "content": [{"type": "text", "text": text}]
            }))
        }
        AgentCommandResult::Error(message) => Err(message),
    }
}

async fn agent_query(
    connection: &vmux_service::client::ServiceConnection,
    query: AgentQuery,
) -> Result<AgentQueryResult, String> {
    let request_id = AgentRequestId::new();
    connection
        .send(&ClientMessage::AgentQuery { request_id, query })
        .await
        .map_err(|error| format!("cannot send query: {error}"))?;
    loop {
        let Some(message) = connection
            .recv()
            .await
            .map_err(|error| format!("cannot read query response: {error}"))?
        else {
            return Err("vmux_service disconnected".to_string());
        };
        match message {
            ServiceMessage::AgentQueryResult {
                request_id: received,
                result,
            } if received == request_id => return Ok(result),
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}

async fn run_agent_query(query: vmux_service::protocol::AgentQuery) -> Result<Value, String> {
    let request_id = vmux_service::protocol::AgentRequestId::new();
    let connection = vmux_service::client::ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    connection
        .send(&ClientMessage::AgentQuery { request_id, query })
        .await
        .map_err(|error| format!("cannot send agent query: {error}"))?;

    loop {
        let Some(message) = connection
            .recv()
            .await
            .map_err(|error| format!("cannot read service response: {error}"))?
        else {
            return Err("vmux_service disconnected".to_string());
        };
        match message {
            ServiceMessage::AgentQueryResult {
                request_id: received,
                result,
            } if received == request_id => {
                return Ok(query_result_to_mcp_response(result));
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}

pub fn query_result_to_mcp_response(result: vmux_service::protocol::AgentQueryResult) -> Value {
    use vmux_service::protocol::AgentQueryResult;
    match result {
        AgentQueryResult::Layout(snapshot) => {
            let text = serde_json::to_string(&snapshot).unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::VaultStatus(snapshot) => {
            let text = serde_json::to_string_pretty(&snapshot).unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::Text(text) => {
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::Settings(settings) => {
            let value = serde_json::Value::try_from(&settings).unwrap_or(serde_json::Value::Null);
            let text = serde_json::to_string(&value).unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::Spaces(spaces) => {
            let text = serde_json::to_string(&spaces).unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::Bookmarks(bookmarks) => {
            let text = serde_json::to_string(&bookmarks).unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::Commands(commands) => {
            let text = serde_json::to_string(&commands).unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::CommandExit { seq, exit } => {
            let exit = exit.map_or_else(|| "null".to_string(), |code| code.to_string());
            json!({
                "content": [{"type": "text", "text": format!("{{\"seq\":{seq},\"exit\":{exit}}}")}]
            })
        }
        AgentQueryResult::RunCompletion { token, exit } => {
            let token = token.map_or_else(|| "null".to_string(), |t| format!("\"{t}\""));
            let exit = exit.map_or_else(|| "null".to_string(), |code| code.to_string());
            json!({
                "content": [{"type": "text", "text": format!("{{\"token\":{token},\"exit\":{exit}}}")}]
            })
        }
        AgentQueryResult::Image {
            path,
            png,
            width,
            height,
        } => {
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD.encode(&png);
            json!({
                "content": [
                    {"type": "text", "text": format!("saved {path} ({width}×{height})")},
                    {"type": "image", "data": data, "mimeType": "image/png"}
                ]
            })
        }
        AgentQueryResult::Recording {
            mp4_path,
            gif_path,
            duration_ms,
            bytes,
            auto_stopped,
        } => {
            let secs = duration_ms as f64 / 1000.0;
            let mut text = format!("recorded {secs:.1}s → {mp4_path} ({bytes} bytes)");
            if let Some(g) = gif_path {
                text.push_str(&format!(" + {g}"));
            }
            if auto_stopped {
                text.push_str(" (auto-stopped)");
            }
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::Error(message) => {
            json!({
                "isError": true,
                "content": [{"type": "text", "text": message}]
            })
        }
    }
}

pub fn tool_error(message: &str) -> Value {
    json!({
        "isError": true,
        "content": [
            {
                "type": "text",
                "text": message
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newline_framing_reads_single_json_message() {
        let mut lines = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n".as_slice();
        let request = read_json_line(&mut lines).unwrap().unwrap();

        assert_eq!(request["method"], "tools/list");
    }

    #[test]
    fn image_query_result_maps_to_text_and_image_blocks() {
        use vmux_service::protocol::AgentQueryResult;
        let resp = query_result_to_mcp_response(AgentQueryResult::Image {
            path: "/tmp/shot.png".into(),
            png: vec![137, 80, 78, 71],
            width: 800,
            height: 600,
        });
        let content = resp["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert!(
            content[0]["text"]
                .as_str()
                .unwrap()
                .contains("/tmp/shot.png")
        );
        assert!(content[0]["text"].as_str().unwrap().contains("800"));
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["mimeType"], "image/png");
        assert_eq!(content[1]["data"], "iVBORw==");
    }

    #[test]
    fn recording_maps_to_text_block() {
        use vmux_service::protocol::AgentQueryResult;
        let v = query_result_to_mcp_response(AgentQueryResult::Recording {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: Some("/tmp/x.gif".into()),
            duration_ms: 7400,
            bytes: 1_000_000,
            auto_stopped: true,
        });
        let text = v["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("/tmp/x.mp4"));
        assert!(text.contains("/tmp/x.gif"));
        assert!(text.contains("auto-stopped"));
        assert!(v.get("isError").is_none());
    }
}
