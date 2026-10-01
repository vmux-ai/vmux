use bevy_app::{App, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use serde_json::{Value, json};
use std::future::Future;
use std::io::{self, BufRead};
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;
use vmux_api::protocol::{
    AgentCommandResult, AgentListCommands, AgentRequest, AgentRequestId, ClientMessage, ProcessId,
    ServiceMessage,
};
use vmux_ecs::{HostShell, JsonArguments, ProcessAnchor};
use vmux_tool::{
    AcpSessionContext, AcpTerminalContext, ToolCall, ToolCatalog, ToolCatalogRequest, ToolCommand,
    ToolCommandFallback, ToolDefinition, ToolDispatchError, ToolDispatchFlush, ToolDispatchSet,
    ToolInvocation, ToolQuery, ToolRegistryPlugin, ToolRequestSet, ToolResolveSet,
};
use vmux_transport::service::ServiceConnection;

use base64::Engine;

pub struct McpPlugin;

impl Plugin for McpPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ToolRegistryPlugin>() {
            app.add_plugins(ToolRegistryPlugin);
        }
        app.add_message::<McpInput>()
            .add_message::<McpOutput>()
            .configure_sets(
                Update,
                (
                    McpSet::Input,
                    McpSet::Route,
                    ToolResolveSet,
                    ToolRequestSet,
                    ToolDispatchSet,
                    ToolDispatchFlush,
                    McpSet::StartTasks,
                    McpSet::BuildResponses,
                    McpSet::Output,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    receive_requests.in_set(McpSet::Route),
                    route_request.in_set(McpSet::Route),
                    (
                        finish_tool_errors,
                        start_list_tools,
                        start_tool_commands,
                        start_tool_queries,
                        bevy_ecs::schedule::ApplyDeferred,
                        start_tasks,
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
pub(crate) enum McpSet {
    Input,
    Route,
    StartTasks,
    BuildResponses,
    Output,
}

#[derive(Component, Default)]
pub struct McpServer {
    next_request_sequence: u64,
}

#[derive(Component, Clone)]
pub(crate) struct McpConfig {
    pub(crate) anchor: Option<ProcessId>,
    pub(crate) acp_session: bool,
    pub(crate) acp_terminals: bool,
    pub(crate) run_block_timeout: Duration,
    pub(crate) shell: String,
}

#[derive(Component, Clone)]
pub(crate) struct McpRuntime(pub(crate) tokio::runtime::Handle);

#[derive(Message)]
pub(crate) struct McpInput(pub(crate) Value);

#[derive(Message)]
pub(crate) struct McpOutput(pub(crate) Value);

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

fn receive_requests(
    mut input: MessageReader<McpInput>,
    mut servers: Query<(&mut McpServer, &McpConfig)>,
    mut commands: Commands,
) {
    let Ok((mut server, config)) = servers.single_mut() else {
        return;
    };
    for McpInput(message) in input.read() {
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        let sequence = server.next_request_sequence;
        server.next_request_sequence += 1;
        commands.spawn((
            McpRequest::new(config.run_block_timeout),
            Name::new(format!("MCP request {sequence}")),
            McpRequestId(id),
            McpMethod(method),
            McpParams(params),
            McpRequestSequence(sequence),
        ));
    }
}

fn route_request(mut commands: Commands, requests: PendingRequests, configs: Query<&McpConfig>) {
    let Ok(config) = configs.single() else {
        return;
    };
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
            request.insert((ToolCatalogRequest, HostShell(config.shell.clone())));
            if config.acp_session {
                request.insert(AcpSessionContext);
            }
            if config.acp_terminals {
                request.insert(AcpTerminalContext);
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
                ToolInvocation,
                ToolCommandFallback,
            ));
            if let Some(anchor) = config.anchor {
                request.insert(ProcessAnchor(anchor));
            }
            if config.acp_session {
                request.insert(AcpSessionContext);
            }
            if config.acp_terminals {
                request.insert(AcpTerminalContext);
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
    errors: Query<(Entity, &ToolDispatchError), Added<ToolDispatchError>>,
) {
    for (entity, error) in &errors {
        commands
            .entity(entity)
            .remove::<ToolInvocation>()
            .remove::<ToolCall>()
            .remove::<ToolDispatchError>()
            .insert(McpReply::Result(Err(error.message().to_string())));
    }
}

fn start_list_tools(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCatalog), Added<ToolCatalog>>,
) {
    for (entity, request) in &requests {
        let mut definitions = request.0.clone();
        commands
            .entity(entity)
            .remove::<ToolCatalogRequest>()
            .remove::<ToolCatalog>()
            .insert(McpExecution::new(async move {
                if let Ok(connection) = ServiceConnection::connect().await
                    && let Ok(ServiceMessage::AgentQueryResult(result)) =
                        agent_query(&connection, AgentRequest::encode(&AgentListCommands)?).await
                    && !result.is_error
                    && let Ok(commands) = serde_json::from_str(&result.content)
                {
                    definitions = ToolDefinition::merge_commands(definitions, commands)?;
                }
                Ok(json!({ "tools": definitions }))
            }));
    }
}

fn start_tool_commands(
    mut commands: Commands,
    requests: Query<(Entity, Option<&ProcessAnchor>, &ToolCommand), Added<ToolCommand>>,
) {
    for (entity, anchor, result) in &requests {
        let mut request = commands.entity(entity);
        request
            .remove::<ToolInvocation>()
            .remove::<ToolCall>()
            .remove::<ToolCommand>();
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
    requests: Query<(Entity, &ToolQuery), Added<ToolQuery>>,
) {
    for (entity, result) in &requests {
        let mut request = commands.entity(entity);
        request
            .remove::<ToolInvocation>()
            .remove::<ToolCall>()
            .remove::<ToolQuery>();
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

fn start_tasks(
    runtimes: Query<&McpRuntime>,
    mut pending: Query<(Entity, &mut McpExecution), Added<McpExecution>>,
    mut commands: Commands,
) {
    let Ok(runtime) = runtimes.single() else {
        return;
    };
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
    replies: Query<(Entity, &McpRequestId, &McpReply)>,
    mut output: MessageWriter<McpOutput>,
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
        output.write(McpOutput(response));
        commands.entity(entity).despawn();
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
    request: AgentRequest,
    anchor: Option<ProcessId>,
) -> Result<Value, String> {
    let request_id = AgentRequestId::new();
    let connection = ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    connection
        .send(&ClientMessage::AgentRequest {
            request_id,
            anchor,
            request,
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

pub fn command_result_to_mcp_response(result: AgentCommandResult) -> Result<Value, String> {
    match result {
        AgentCommandResult::Ok => Ok(json!({
            "content": [{"type": "text", "text": "ok"}]
        })),
        AgentCommandResult::Text(text) => Ok(json!({
            "content": [{"type": "text", "text": text}]
        })),
        AgentCommandResult::Error(message) => Err(message),
    }
}

async fn agent_query(
    connection: &ServiceConnection,
    query: AgentRequest,
) -> Result<ServiceMessage, String> {
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
        if query_response_request_id(&message) == Some(request_id) {
            return Ok(message);
        }
        if let ServiceMessage::Error { message } = message {
            return Err(message);
        }
    }
}

fn query_response_request_id(message: &ServiceMessage) -> Option<AgentRequestId> {
    match message {
        ServiceMessage::AgentQueryResult(result) => Some(result.request_id),
        _ => None,
    }
}

async fn run_agent_query(query: AgentRequest) -> Result<Value, String> {
    let connection = ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    let response = agent_query(&connection, query).await?;
    Ok(query_response_to_mcp_response(response))
}

pub fn query_response_to_mcp_response(response: ServiceMessage) -> Value {
    match response {
        ServiceMessage::AgentQueryResult(result) => {
            if result.is_error {
                return tool_error(&result.content);
            }
            let mut content = vec![json!({"type": "text", "text": result.content})];
            if let Some(image) = result.image {
                let data = base64::engine::general_purpose::STANDARD.encode(&image.png);
                content.push(json!({
                    "type": "image",
                    "data": data,
                    "mimeType": "image/png"
                }));
            }
            json!({"content": content})
        }
        _ => tool_error("unexpected agent query response"),
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
        let resp = query_response_to_mcp_response(ServiceMessage::AgentQueryResult(
            vmux_api::protocol::AgentQueryResult {
                request_id: AgentRequestId::new(),
                content: "saved /tmp/shot.png (800×600)".to_string(),
                is_error: false,
                image: Some(vmux_api::protocol::AgentImage {
                    path: "/tmp/shot.png".into(),
                    png: vec![137, 80, 78, 71],
                    width: 800,
                    height: 600,
                }),
            },
        ));
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
        let v = query_response_to_mcp_response(ServiceMessage::AgentQueryResult(
            vmux_api::protocol::AgentQueryResult {
                request_id: AgentRequestId::new(),
                content: "recorded 7.4s → /tmp/x.mp4 (1000000 bytes) + /tmp/x.gif (auto-stopped)"
                    .to_string(),
                is_error: false,
                image: None,
            },
        ));
        let text = v["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("/tmp/x.mp4"));
        assert!(text.contains("/tmp/x.gif"));
        assert!(text.contains("auto-stopped"));
        assert!(v.get("isError").is_none());
    }
}
