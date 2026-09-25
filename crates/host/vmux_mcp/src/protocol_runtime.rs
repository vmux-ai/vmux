use bevy_app::{App, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use serde_json::{Value, json};
use std::future::Future;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vmux_client::protocol::{
    AgentCommand, AgentQuery, AgentQueryResult, AgentRequestId, ClientMessage, FileTouchKind,
    ServiceMessage,
};

const RUN_PROCESS_MATERIALIZE_TIMEOUT: Duration = Duration::from_secs(2);
const RUN_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub struct McpPlugin {
    config: McpConfig,
}

impl McpPlugin {
    pub fn new(
        anchor: Option<vmux_client::protocol::ProcessId>,
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
                    start_read_file_tools,
                    start_grep_tools,
                    start_vault_status_tools,
                    start_tool_commands,
                    start_tool_queries,
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
        anchor: Option<vmux_client::protocol::ProcessId>,
        acp_session: bool,
        acp_terminals: bool,
        run_block_timeout: Duration,
        shell: String,
    ) -> Self {
        let mut app = App::new();
        app.add_plugins((
            crate::tool::BuiltinToolPlugin,
            McpPlugin::new(anchor, acp_session, acp_terminals, run_block_timeout, shell),
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
                McpRequest,
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
    anchor: Option<vmux_client::protocol::ProcessId>,
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
pub(crate) struct McpRequest;

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

impl McpTask {
    fn spawn(
        runtime: &McpRuntime,
        future: impl Future<Output = Result<Value, String>> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        drop(runtime.0.spawn(async move {
            let _ = sender.send(future.await);
        }));
        Self(receiver)
    }
}

#[derive(Component)]
struct ListToolsExecution {
    definitions: Vec<crate::tool::ToolDefinition>,
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

pub async fn run_stdio(
    anchor: Option<vmux_client::protocol::ProcessId>,
    acp_session: bool,
    acp_terminals: bool,
    run_block_timeout: Duration,
    shell: String,
) -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    let mut server = McpServer::new(anchor, acp_session, acp_terminals, run_block_timeout, shell);

    while let Some(message) = read_json_line(&mut reader)? {
        if let Some(response) = server.handle(message).await {
            serde_json::to_writer(&mut writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
async fn handle_message(
    message: Value,
    anchor: Option<vmux_client::protocol::ProcessId>,
    acp_session: bool,
    acp_terminals: bool,
    run_block_timeout: Duration,
    shell: &str,
) -> Option<Value> {
    McpServer::new(
        anchor,
        acp_session,
        acp_terminals,
        run_block_timeout,
        shell.to_string(),
    )
    .handle(message)
    .await
}

fn route_request(
    mut commands: Commands,
    requests: PendingRequests,
    tools: crate::tool::ToolRegistry,
    config: Single<&McpConfig>,
) {
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
            let definitions =
                tools.definitions(config.acp_session, config.acp_terminals, &config.shell);
            commands
                .entity(entity)
                .insert(ListToolsExecution { definitions });
        }
        "tools/call" => {
            let Some(name) = params.0.get("name").and_then(Value::as_str) else {
                commands
                    .entity(entity)
                    .insert(McpReply::Result(Err("tools/call missing name".to_string())));
                return;
            };
            let normalized = crate::tool::canonical_tool_name(name);
            let arguments = params
                .0
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match tools.call(
                normalized,
                arguments,
                config.anchor,
                &config.shell,
                crate::tool::ToolCallPolicy::mcp(config.acp_session, config.acp_terminals),
            ) {
                Ok(call) => {
                    commands.entity(entity).insert(call);
                }
                Err(message) => {
                    commands
                        .entity(entity)
                        .insert(McpReply::Result(Err(message)));
                }
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
            .remove::<crate::tool::ToolCall>()
            .remove::<crate::tool::ToolDispatchError>()
            .insert(McpReply::Result(Err(error.message().to_string())));
    }
}

fn start_list_tools(
    mut commands: Commands,
    runtime: Single<&McpRuntime>,
    requests: Query<(Entity, &ListToolsExecution), Added<ListToolsExecution>>,
) {
    for (entity, request) in &requests {
        let mut definitions = request.definitions.clone();
        commands
            .entity(entity)
            .remove::<ListToolsExecution>()
            .insert(McpTask::spawn(&runtime, async move {
                if let Ok(connection) = vmux_client::client::ServiceConnection::connect().await
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

fn start_read_file_tools(
    mut commands: Commands,
    runtime: Single<&McpRuntime>,
    requests: Query<
        (Entity, &crate::tool::ReadFileExecution),
        Added<crate::tool::ReadFileExecution>,
    >,
) {
    for (entity, request) in &requests {
        let path = request.path.clone();
        let offset = request.offset.map(std::num::NonZeroU32::get);
        let limit = request.limit;
        let anchor = request.anchor;
        commands
            .entity(entity)
            .remove::<crate::tool::ToolCall>()
            .remove::<crate::tool::ReadFileExecution>()
            .insert(McpTask::spawn(
                &runtime,
                read_file_result(path, offset, limit, anchor),
            ));
    }
}

fn start_grep_tools(
    mut commands: Commands,
    runtime: Single<&McpRuntime>,
    requests: Query<(Entity, &crate::tool::GrepExecution), Added<crate::tool::GrepExecution>>,
) {
    for (entity, request) in &requests {
        let query = request.query.clone();
        let path = request.path.clone();
        let anchor = request.anchor;
        commands
            .entity(entity)
            .remove::<crate::tool::ToolCall>()
            .remove::<crate::tool::GrepExecution>()
            .insert(McpTask::spawn(&runtime, grep_result(query, path, anchor)));
    }
}

fn start_vault_status_tools(
    mut commands: Commands,
    runtime: Single<&McpRuntime>,
    requests: Query<Entity, Added<crate::tool::VaultStatusExecution>>,
) {
    for entity in &requests {
        commands
            .entity(entity)
            .remove::<crate::tool::ToolCall>()
            .remove::<crate::tool::VaultStatusExecution>()
            .insert(McpTask::spawn(
                &runtime,
                run_agent_query(AgentQuery::VaultStatus),
            ));
    }
}

fn start_tool_commands(
    mut commands: Commands,
    runtime: Single<&McpRuntime>,
    requests: Query<
        (Entity, &crate::tool::ToolCall, &crate::tool::ToolCommand),
        Added<crate::tool::ToolCommand>,
    >,
    config: Single<&McpConfig>,
) {
    for (entity, call, result) in &requests {
        let mut request = commands.entity(entity);
        request
            .remove::<crate::tool::ToolCall>()
            .remove::<crate::tool::ToolCommand>();
        let command = match result.0.clone() {
            Ok(command) => command,
            Err(message) => {
                request.insert(McpReply::Result(Err(message)));
                continue;
            }
        };
        let name = call.name.clone();
        let arguments = call.arguments.clone();
        let anchor = call.anchor;
        let run_block_timeout = config.run_block_timeout;
        request.insert(McpTask::spawn(&runtime, async move {
            if name == "open_file" {
                let path = arguments
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or("open_file.path is required")?;
                if !Path::new(path).is_absolute() {
                    return Err("open_file.path must be an absolute path".to_string());
                }
                scoped_existing_path(anchor, Path::new(path), "open_file").await?;
            }

            match command {
                command @ AgentCommand::Run { .. }
                | command @ AgentCommand::RunWithPlacementOverride { .. } => {
                    run_blocking(command, run_block_timeout).await
                }
                command => run_agent_command(command, anchor).await,
            }
        }));
    }
}

fn start_tool_queries(
    mut commands: Commands,
    runtime: Single<&McpRuntime>,
    requests: Query<(Entity, &crate::tool::ToolQuery), Added<crate::tool::ToolQuery>>,
) {
    for (entity, result) in &requests {
        let mut request = commands.entity(entity);
        request
            .remove::<crate::tool::ToolCall>()
            .remove::<crate::tool::ToolQuery>();
        let query = match result.0.clone() {
            Ok(query) => query,
            Err(message) => {
                request.insert(McpReply::Result(Err(message)));
                continue;
            }
        };
        request.insert(McpTask::spawn(&runtime, run_agent_query(query)));
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

async fn agent_working_directory(
    anchor: Option<vmux_client::protocol::ProcessId>,
) -> Result<PathBuf, String> {
    let Some(anchor) = anchor else {
        return std::env::current_dir()
            .and_then(|path| path.canonicalize())
            .map_err(|error| format!("cannot resolve current directory: {error}"));
    };
    let connection = vmux_client::client::ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    match agent_query(&connection, AgentQuery::WorkingDirectory { anchor }).await? {
        AgentQueryResult::Text(path) => PathBuf::from(path)
            .canonicalize()
            .map_err(|error| format!("cannot resolve agent working directory: {error}")),
        AgentQueryResult::Error(message) => Err(message),
        _ => Err("unexpected agent working directory response".to_string()),
    }
}

fn resolve_scoped_existing_path(scope: &Path, requested: &Path) -> Option<PathBuf> {
    let path = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        scope.join(requested)
    };
    let path = path.canonicalize().ok()?;
    path.starts_with(scope).then_some(path)
}

async fn scoped_existing_path(
    anchor: Option<vmux_client::protocol::ProcessId>,
    requested: &Path,
    tool: &str,
) -> Result<PathBuf, String> {
    let scope = agent_working_directory(anchor).await?;
    resolve_scoped_existing_path(&scope, requested).ok_or_else(|| {
        format!(
            "{tool}: path is outside the selected project; call select_project and wait for user approval first"
        )
    })
}

async fn read_file_result(
    requested: String,
    offset: Option<u32>,
    limit: Option<usize>,
    anchor: Option<vmux_client::protocol::ProcessId>,
) -> Result<Value, String> {
    if !Path::new(&requested).is_absolute() {
        return Err("read_file.path must be an absolute path".to_string());
    }
    let path = scoped_existing_path(anchor, Path::new(&requested), "read_file").await?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("read_file: {e}"))?;
    if !meta.is_file() {
        return Err("read_file: not a regular file".to_string());
    }
    let path_text = path.to_string_lossy();
    let text =
        read_lines_bounded(&path_text, offset, limit).map_err(|e| format!("read_file: {e}"))?;
    if let Some(anchor) = anchor {
        let _ = run_agent_command(
            AgentCommand::FileTouched {
                anchor,
                path: path.to_string_lossy().into_owned(),
                line: offset,
                col: None,
                end_col: None,
                kind: FileTouchKind::Read,
            },
            Some(anchor),
        )
        .await;
    }
    Ok(json!({ "content": [{"type": "text", "text": text}] }))
}

const GREP_MAX_FILES: usize = 10;
const GREP_MAX_LINES: usize = 200;

async fn grep_result(
    query: String,
    requested: Option<String>,
    anchor: Option<vmux_client::protocol::ProcessId>,
) -> Result<Value, String> {
    let requested = requested.as_deref().unwrap_or(".");
    let search_path = scoped_existing_path(anchor, Path::new(requested), "grep").await?;

    use std::io::{BufRead, Read};
    let mut child = std::process::Command::new("rg")
        .args(["--json", "--", &query])
        .arg(&search_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("grep: cannot run rg (is ripgrep installed?): {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("grep: failed to capture rg output")?;
    let mut stderr_pipe = child.stderr.take();
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(stderr) = stderr_pipe.as_mut() {
            let _ = stderr.read_to_string(&mut buf);
        }
        buf
    });

    let mut order: Vec<String> = Vec::new();
    let mut first_line: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut first_cols: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    let mut lines_out: Vec<String> = Vec::new();
    let mut search_matches = Vec::new();
    let mut capped = false;
    for line in std::io::BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("match") {
            continue;
        }
        let Some(data) = v.get("data") else { continue };
        let path = data
            .get("path")
            .and_then(|p| p.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if path.is_empty() {
            continue;
        }
        let lineno = data.get("line_number").and_then(Value::as_u64).unwrap_or(0) as u32;
        let raw = data
            .get("lines")
            .and_then(|l| l.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let first_match = data
            .get("submatches")
            .and_then(|s| s.as_array())
            .and_then(|a| a.first());
        let cols = first_match.map(|sm| {
            let start = sm.get("start").and_then(Value::as_u64).unwrap_or(0) as usize;
            let end = sm.get("end").and_then(Value::as_u64).unwrap_or(0) as usize;
            (byte_to_utf16(raw, start), byte_to_utf16(raw, end))
        });
        if !first_line.contains_key(path) {
            first_line.insert(path.to_string(), lineno);
            if let Some(cols) = cols {
                first_cols.insert(path.to_string(), cols);
            }
            order.push(path.to_string());
        }
        if lines_out.len() < GREP_MAX_LINES {
            lines_out.push(format!("{path}:{lineno}: {}", raw.trim_end()));
            let (col, end_col) = cols.unwrap_or((0, 0));
            search_matches.push((
                path.to_string(),
                lineno,
                col,
                end_col,
                raw.trim_end().to_string(),
            ));
        }
        if lines_out.len() >= GREP_MAX_LINES || order.len() >= GREP_MAX_FILES {
            capped = true;
            break;
        }
    }
    if capped {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| format!("grep: {e}"))?;
    let stderr = stderr_handle.join().unwrap_or_default();

    if order.is_empty() {
        if !capped && status.code() != Some(0) && status.code() != Some(1) {
            let err = stderr.trim();
            return Err(if err.is_empty() {
                "grep: rg failed".to_string()
            } else {
                format!("grep: {err}")
            });
        }
        return Ok(
            json!({ "content": [{"type": "text", "text": format!("no matches for {query:?}")}] }),
        );
    }

    if let Some(anchor) = anchor {
        if let Some(file) = order.first()
            && let Ok(abs) = std::fs::canonicalize(file)
        {
            let _ = run_agent_command(
                AgentCommand::FileTouched {
                    anchor,
                    path: abs.to_string_lossy().to_string(),
                    line: first_line.get(file).copied(),
                    col: first_cols.get(file).map(|c| c.0),
                    end_col: first_cols.get(file).map(|c| c.1),
                    kind: FileTouchKind::Read,
                },
                Some(anchor),
            )
            .await;
        }
        let mut canonical_paths = std::collections::HashMap::new();
        let matches = search_matches
            .into_iter()
            .filter_map(|(path, line, col, end_col, preview)| {
                let canonical = canonical_paths
                    .entry(path.clone())
                    .or_insert_with(|| std::fs::canonicalize(&path).ok())
                    .clone()?;
                Some(vmux_client::protocol::FileSearchMatch {
                    path: canonical.to_string_lossy().into_owned(),
                    line,
                    col,
                    end_col,
                    preview,
                })
            })
            .collect::<Vec<_>>();
        if !matches.is_empty() {
            let _ = run_agent_command(
                AgentCommand::FileSearch {
                    anchor,
                    root: search_path.to_string_lossy().into_owned(),
                    query: query.clone(),
                    matches,
                },
                Some(anchor),
            )
            .await;
        }
    }

    let mut text = lines_out.join("\n");
    if capped {
        text.push_str(&format!(
            "\n\u{2026} results truncated at {GREP_MAX_FILES} files / {GREP_MAX_LINES} lines; refine the query"
        ));
    } else if order.len() > GREP_MAX_FILES {
        text.push_str(&format!(
            "\n\u{2026} opened first {GREP_MAX_FILES} of {} matching files",
            order.len()
        ));
    }
    Ok(json!({ "content": [{"type": "text", "text": text}] }))
}

const READ_FILE_DEFAULT_LINES: usize = 2000;
const READ_FILE_MAX_LINES: usize = 50_000;

fn read_lines_bounded(
    path: &str,
    offset: Option<u32>,
    limit: Option<usize>,
) -> std::io::Result<String> {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let start = offset.map(|o| o.saturating_sub(1) as usize).unwrap_or(0);
    let take = limit
        .map(|l| l.min(READ_FILE_MAX_LINES))
        .unwrap_or(READ_FILE_DEFAULT_LINES);
    let mut out: Vec<String> = Vec::new();
    for line in reader.lines().skip(start).take(take) {
        out.push(line?);
    }
    Ok(out.join("\n"))
}

fn byte_to_utf16(line: &str, byte: usize) -> u32 {
    let mut idx = byte.min(line.len());
    while idx > 0 && !line.is_char_boundary(idx) {
        idx -= 1;
    }
    line[..idx].encode_utf16().count() as u32
}

fn output_since(baseline: &str, final_text: &str) -> String {
    final_text
        .strip_prefix(baseline)
        .unwrap_or(final_text)
        .trim_matches('\n')
        .trim_end()
        .to_string()
}

fn run_done_token(request_id: AgentRequestId) -> String {
    let mut token = String::with_capacity(32);
    for byte in request_id.0 {
        use std::fmt::Write;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

fn blocking_run_with_marker(mut run: AgentCommand, request_id: AgentRequestId) -> AgentCommand {
    match &mut run {
        AgentCommand::Run { done_marker, .. }
        | AgentCommand::RunWithPlacementOverride { done_marker, .. } => {
            *done_marker = Some(run_done_token(request_id));
        }
        _ => {}
    }
    run
}

fn run_result(
    pid: &str,
    exit: Option<i32>,
    output: &str,
    timed_out: bool,
    run_block_timeout: Duration,
) -> Value {
    let mut text = format!("terminal: {pid}\n");
    match exit {
        Some(code) => text.push_str(&format!("exit: {code}\n")),
        None if timed_out => text.push_str(&format!(
            "note: still running after {}s; call read_terminal({pid}) to read more\n",
            run_block_timeout.as_secs()
        )),
        None => {}
    }
    text.push_str("output:\n");
    text.push_str(output);
    json!({ "content": [{"type": "text", "text": text}] })
}

fn run_completion_exit(
    result: AgentQueryResult,
    token: &str,
    process_id: vmux_client::protocol::ProcessId,
    allow_missing_process: bool,
) -> Result<Option<i32>, String> {
    match result {
        AgentQueryResult::RunCompletion {
            token: Some(done_token),
            exit: Some(exit),
        } if done_token == token => Ok(Some(exit)),
        AgentQueryResult::RunCompletion { .. } => Ok(None),
        AgentQueryResult::Error(message)
            if allow_missing_process && message == format!("process not found: {process_id}") =>
        {
            Ok(None)
        }
        AgentQueryResult::Error(message) => Err(message),
        other => Err(format!(
            "run: unexpected run-completion result for {process_id}: {other:?}"
        )),
    }
}

async fn run_blocking(run: AgentCommand, run_block_timeout: Duration) -> Result<Value, String> {
    let connection = vmux_client::client::ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;

    let request_id = AgentRequestId::new();
    let token = run_done_token(request_id);
    let run = blocking_run_with_marker(run, request_id);
    connection
        .send(&ClientMessage::AgentCommand {
            request_id,
            anchor: None,
            command: run,
        })
        .await
        .map_err(|error| format!("cannot send run command: {error}"))?;

    let pid = loop {
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
                use vmux_client::protocol::AgentCommandResult;
                match result {
                    AgentCommandResult::Text(pid) => break pid,
                    AgentCommandResult::Error(message) => return Err(message),
                    other => return Err(format!("run: unexpected result: {other:?}")),
                }
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    };

    let process_id = pid
        .parse::<vmux_client::protocol::ProcessId>()
        .map_err(|_| format!("run: service returned an invalid terminal id: {pid}"))?;

    let start = Instant::now();
    let baseline_text = read_full_text(&connection, process_id).await;
    let deadline = start + run_block_timeout;
    let materialize_deadline = start + RUN_PROCESS_MATERIALIZE_TIMEOUT;
    let mut process_materialized = false;
    loop {
        let result = agent_query(&connection, AgentQuery::RunCompletion { process_id }).await?;
        let materialized_now = matches!(&result, AgentQueryResult::RunCompletion { .. });
        let allow_missing_process = !process_materialized && Instant::now() < materialize_deadline;
        let exit = run_completion_exit(result, &token, process_id, allow_missing_process)?;
        process_materialized |= materialized_now;
        if let Some(exit) = exit {
            let final_text = read_full_text(&connection, process_id).await;
            let output = output_since(&baseline_text, &final_text);
            return Ok(run_result(
                &pid,
                Some(exit),
                &output,
                false,
                run_block_timeout,
            ));
        }
        if Instant::now() >= deadline {
            let final_text = read_full_text(&connection, process_id).await;
            let output = output_since(&baseline_text, &final_text);
            return Ok(run_result(&pid, None, &output, true, run_block_timeout));
        }
        tokio::time::sleep(RUN_POLL_INTERVAL).await;
    }
}

async fn agent_query(
    connection: &vmux_client::client::ServiceConnection,
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

async fn read_full_text(
    connection: &vmux_client::client::ServiceConnection,
    process_id: vmux_client::protocol::ProcessId,
) -> String {
    match agent_query(connection, AgentQuery::ReadTerminalFull { process_id }).await {
        Ok(AgentQueryResult::Text(text)) => text,
        _ => String::new(),
    }
}

async fn run_agent_command(
    command: AgentCommand,
    anchor: Option<vmux_client::protocol::ProcessId>,
) -> Result<Value, String> {
    let request_id = vmux_client::protocol::AgentRequestId::new();
    let connection = vmux_client::client::ServiceConnection::connect()
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
    result: vmux_client::protocol::AgentCommandResult,
) -> Result<Value, String> {
    use vmux_client::protocol::AgentCommandResult;
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

async fn run_agent_query(query: vmux_client::protocol::AgentQuery) -> Result<Value, String> {
    let request_id = vmux_client::protocol::AgentRequestId::new();
    let connection = vmux_client::client::ServiceConnection::connect()
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

pub fn query_result_to_mcp_response(result: vmux_client::protocol::AgentQueryResult) -> Value {
    use vmux_client::protocol::AgentQueryResult;
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
    fn read_lines_bounded_offset_and_limit() {
        let dir = std::env::temp_dir().join(format!("vmux-mcp-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.txt");
        std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();
        let p = path.to_str().unwrap();
        assert_eq!(read_lines_bounded(p, None, None).unwrap(), "a\nb\nc\nd\ne");
        assert_eq!(read_lines_bounded(p, Some(2), Some(2)).unwrap(), "b\nc");
        assert_eq!(read_lines_bounded(p, Some(4), None).unwrap(), "d\ne");
        assert_eq!(read_lines_bounded(p, Some(99), None).unwrap(), "");
        assert_eq!(
            read_lines_bounded(p, Some(1), Some(100)).unwrap(),
            "a\nb\nc\nd\ne"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scoped_paths_reject_files_outside_the_selected_project() {
        let scope = tempfile::tempdir().unwrap();
        let inside = scope.path().join("inside.txt");
        std::fs::write(&inside, "inside").unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let scope = scope.path().canonicalize().unwrap();

        assert_eq!(
            resolve_scoped_existing_path(&scope, &inside),
            Some(inside.canonicalize().unwrap())
        );
        assert_eq!(resolve_scoped_existing_path(&scope, outside.path()), None);
    }

    #[test]
    fn byte_to_utf16_converts_multibyte_offsets() {
        let line = "aé😀b";
        assert_eq!(byte_to_utf16(line, 0), 0);
        assert_eq!(byte_to_utf16(line, 1), 1);
        assert_eq!(byte_to_utf16(line, 2), 1);
        assert_eq!(byte_to_utf16(line, 3), 2);
        assert_eq!(byte_to_utf16(line, 7), 4);
        assert_eq!(byte_to_utf16(line, 999), 5);
    }

    #[test]
    fn newline_framing_reads_single_json_message() {
        let mut lines = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n".as_slice();
        let request = read_json_line(&mut lines).unwrap().unwrap();

        assert_eq!(request["method"], "tools/list");
    }

    #[tokio::test]
    async fn acp_tools_call_rejects_hidden_terminal_tools() {
        for name in ["run", "read_terminal"] {
            let response = handle_message(
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": { "name": name, "arguments": {} }
                }),
                None,
                true,
                true,
                Duration::from_secs(50),
                "",
            )
            .await
            .unwrap();

            assert_eq!(response["result"]["isError"], true);
            assert_eq!(
                response["result"]["content"][0]["text"],
                format!("tool {name} is unavailable for ACP sessions")
            );
        }
    }

    #[tokio::test]
    async fn acp_tools_call_rejects_resume_in_acp() {
        let response = handle_message(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "resume_in_acp", "arguments": {} }
            }),
            None,
            true,
            false,
            Duration::from_secs(50),
            "",
        )
        .await
        .unwrap();

        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["content"][0]["text"],
            "tool resume_in_acp is unavailable for ACP sessions"
        );
    }

    #[test]
    fn image_query_result_maps_to_text_and_image_blocks() {
        use vmux_client::protocol::AgentQueryResult;
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
        use vmux_client::protocol::AgentQueryResult;
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

    #[test]
    fn output_since_returns_appended_tail() {
        let baseline = "prompt$ ";
        let final_text = "prompt$ ls\nfile_a\nfile_b\nprompt$ ";
        assert_eq!(
            output_since(baseline, final_text),
            "ls\nfile_a\nfile_b\nprompt$"
        );
    }

    #[test]
    fn output_since_falls_back_to_full_when_prefix_shifted() {
        let baseline = "old prompt$ ";
        let final_text = "different\noutput here";
        assert_eq!(output_since(baseline, final_text), "different\noutput here");
    }

    #[test]
    fn run_result_shapes_text() {
        let timeout = Duration::from_secs(600);
        let done = run_result("pid7", Some(1), "boom", false, timeout);
        let text = done["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("terminal: pid7"));
        assert!(text.contains("exit: 1"));
        assert!(text.contains("output:\nboom"));

        let timed_out = run_result("pid7", None, "partial", true, timeout);
        let text = timed_out["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("still running"));
        assert!(text.contains("600s"));
        assert!(text.contains("read_terminal(pid7)"));
    }

    #[test]
    fn blocking_run_sets_done_marker_token() {
        let request_id = AgentRequestId([7; 16]);
        let anchor = vmux_client::protocol::ProcessId::new();
        let run = AgentCommand::Run {
            anchor,
            command: "git status".into(),
            direction: vmux_client::protocol::AgentPaneDirection::Right,
            focus: false,
            beside: None,
            mode: vmux_client::protocol::PlacementMode::Auto,
            terminal: None,
            done_marker: None,
        };

        let marked = blocking_run_with_marker(run, request_id);

        match marked {
            AgentCommand::Run { done_marker, .. } => {
                assert_eq!(done_marker, Some(run_done_token(request_id)));
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn blocking_placement_override_run_sets_done_marker_token() {
        let request_id = AgentRequestId([9; 16]);
        let run = AgentCommand::RunWithPlacementOverride {
            anchor: vmux_client::protocol::ProcessId::new(),
            command: "git status".into(),
            direction: vmux_client::protocol::AgentPaneDirection::Bottom,
            focus: false,
            beside: None,
            mode: vmux_client::protocol::PlacementMode::Split,
            terminal: None,
            done_marker: None,
        };

        let marked = blocking_run_with_marker(run, request_id);

        match marked {
            AgentCommand::RunWithPlacementOverride { done_marker, .. } => {
                assert_eq!(done_marker, Some(run_done_token(request_id)));
            }
            _ => panic!("expected run placement override command"),
        }
    }

    #[test]
    fn blocking_run_waits_for_new_terminal_process_to_materialize() {
        let process_id = vmux_client::protocol::ProcessId::new();
        let result = AgentQueryResult::Error(format!("process not found: {process_id}"));

        assert_eq!(
            run_completion_exit(result, "token", process_id, true).unwrap(),
            None
        );
    }

    #[test]
    fn blocking_run_surfaces_process_missing_after_startup_grace() {
        let process_id = vmux_client::protocol::ProcessId::new();
        let message = format!("process not found: {process_id}");
        let result = AgentQueryResult::Error(message.clone());

        assert_eq!(
            run_completion_exit(result, "token", process_id, false),
            Err(message)
        );
    }
}
