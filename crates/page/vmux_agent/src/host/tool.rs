use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vmux_api::protocol::{
    AgentCommand, AgentPaneDirection, AgentQuery, AgentRequestId, AgentRunCompletion,
    ClientMessage, PlacementMode, ProcessId, ServiceMessage,
};
use vmux_core::{HostShell, JsonArguments, ProcessAnchor};
use vmux_mcp::protocol::{McpExecution, McpRequest};
use vmux_service::client::ServiceConnection;
use vmux_tool::{
    AddedTool, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
    ToolRequestSet,
};

const RUN_PROCESS_MATERIALIZE_TIMEOUT: Duration = Duration::from_secs(2);
const RUN_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub struct WorkspaceToolPlugin;

impl Plugin for WorkspaceToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::<WorkspaceTool>::new(include_str!(
            "tool.ron"
        )))
        .add_systems(Update, parse.in_set(ToolRequestSet))
        .add_systems(
            Update,
            (
                open_page,
                open_file,
                resume_in_acp,
                run,
                request_user_choice,
                select_project,
                create_worktree,
                read_terminal,
            )
                .in_set(ToolDispatchSet),
        );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WorkspaceTool {
    OpenPage,
    OpenFile,
    ResumeInAcp,
    Run,
    RequestUserChoice,
    SelectProject,
    CreateWorktree,
    ReadTerminal,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PaneDirection {
    Top,
    Right,
    Bottom,
    Left,
}

impl From<PaneDirection> for AgentPaneDirection {
    fn from(value: PaneDirection) -> Self {
        match value {
            PaneDirection::Top => Self::Top,
            PaneDirection::Right => Self::Right,
            PaneDirection::Bottom => Self::Bottom,
            PaneDirection::Left => Self::Left,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RunMode {
    Auto,
    Split,
    Stack,
}

impl From<RunMode> for PlacementMode {
    fn from(value: RunMode) -> Self {
        match value {
            RunMode::Auto => Self::Auto,
            RunMode::Split => Self::Split,
            RunMode::Stack => Self::Stack,
        }
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenPageArgs {
    url: String,
    direction: Option<PaneDirection>,
    #[serde(default)]
    focus: bool,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenFileArgs {
    path: String,
    direction: Option<PaneDirection>,
    #[serde(default)]
    focus: bool,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunArgs {
    command: String,
    shell: Option<String>,
    direction: Option<PaneDirection>,
    #[serde(default)]
    focus: bool,
    terminal: Option<String>,
    beside: Option<String>,
    mode: Option<RunMode>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateWorktreeArgs {
    branch: Option<String>,
    path: Option<String>,
    task: Option<String>,
    #[serde(default)]
    create: bool,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestUserChoiceArgs {
    question: String,
    options: Vec<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectProjectArgs {
    path: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadTerminalArgs {
    terminal: String,
}

fn resume_in_acp(
    mut commands: Commands,
    calls: Query<(Entity, &Name, Option<&ProcessAnchor>, &WorkspaceTool), AddedTool<WorkspaceTool>>,
) {
    for (request, name, anchor, tool) in &calls {
        if *tool != WorkspaceTool::ResumeInAcp {
            continue;
        }
        let result = ProcessAnchor::required(anchor, name.as_str())
            .map(|anchor| AgentCommand::ResumeInAcp { anchor });
        commands.entity(request).insert(ToolCommand(result));
    }
}

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments, &WorkspaceTool), AddedTool<WorkspaceTool>>,
) {
    for (request, name, arguments, tool) in &calls {
        let parsed = match tool {
            WorkspaceTool::ResumeInAcp => continue,
            WorkspaceTool::OpenPage => arguments.parse::<OpenPageArgs>(name.as_str()).map(|args| {
                commands.entity(request).insert(args);
            }),
            WorkspaceTool::OpenFile => arguments.parse::<OpenFileArgs>(name.as_str()).map(|args| {
                commands.entity(request).insert(args);
            }),
            WorkspaceTool::Run => arguments.parse::<RunArgs>(name.as_str()).map(|args| {
                commands.entity(request).insert(args);
            }),
            WorkspaceTool::RequestUserChoice => arguments
                .parse::<RequestUserChoiceArgs>(name.as_str())
                .map(|args| {
                    commands.entity(request).insert(args);
                }),
            WorkspaceTool::SelectProject => arguments
                .parse::<SelectProjectArgs>(name.as_str())
                .map(|args| {
                    commands.entity(request).insert(args);
                }),
            WorkspaceTool::CreateWorktree => arguments
                .parse::<CreateWorktreeArgs>(name.as_str())
                .map(|args| {
                    commands.entity(request).insert(args);
                }),
            WorkspaceTool::ReadTerminal => {
                arguments
                    .parse::<ReadTerminalArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    })
            }
        };
        if let Err(message) = parsed {
            commands
                .entity(request)
                .insert(ToolDispatchError::new(message));
        }
    }
}

fn open_page(
    mut commands: Commands,
    requests: Query<(Entity, &Name, Option<&ProcessAnchor>, &OpenPageArgs), Added<OpenPageArgs>>,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| {
            if args.url.trim().is_empty() {
                return Err("open_page.url is empty".to_string());
            }
            Ok(AgentCommand::OpenBeside {
                anchor,
                direction: args.direction.map(Into::into),
                url: args.url.clone(),
                focus: args.focus,
            })
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn open_file(
    mut commands: Commands,
    requests: Query<(Entity, &Name, Option<&ProcessAnchor>, &OpenFileArgs), Added<OpenFileArgs>>,
    protocol_requests: Query<&McpRequest>,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| {
            let path = args.path.trim();
            if path.is_empty() {
                return Err("open_file.path is empty".to_string());
            }
            let url = if path.starts_with("file:") {
                path.to_string()
            } else {
                format!("file://{path}")
            };
            Ok(AgentCommand::OpenBeside {
                anchor,
                direction: args.direction.map(Into::into),
                url,
                focus: args.focus,
            })
        });
        match command {
            Ok(command) if protocol_requests.contains(entity) => {
                let requested = args.path.clone();
                let anchor = anchor.map(|anchor| anchor.0);
                commands
                    .entity(entity)
                    .insert(McpExecution::new(async move {
                        if !Path::new(&requested).is_absolute() {
                            return Err("open_file.path must be an absolute path".to_string());
                        }
                        scoped_existing_path(anchor, Path::new(&requested), "open_file").await?;
                        run_agent_command(command, anchor).await
                    }));
            }
            command => {
                commands.entity(entity).insert(ToolCommand(command));
            }
        }
    }
}

fn run(
    mut commands: Commands,
    requests: Query<
        (
            Entity,
            &Name,
            Option<&ProcessAnchor>,
            Option<&HostShell>,
            &RunArgs,
        ),
        Added<RunArgs>,
    >,
    protocol_requests: Query<&McpRequest>,
) {
    for (entity, name, anchor, host_shell, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| {
            let placement_override =
                args.mode.is_some() || args.direction.is_some() || args.beside.is_some();
            let mut command = args.command.clone();
            if command.trim().is_empty() {
                return Err("run.command is empty".to_string());
            }
            if let Some(interpreter) = args.shell.as_ref().filter(|value| !value.trim().is_empty())
            {
                command = vmux_mcp::host_quote::HostQuote::handing_to(
                    host_shell.map_or("", |shell| shell.0.as_str()),
                    interpreter,
                    &command,
                )?;
            }
            let direction = args
                .direction
                .map(Into::into)
                .unwrap_or(AgentPaneDirection::Right);
            let terminal = ProcessTarget::parse(args.terminal.clone(), "run.terminal", "terminal")?;
            let beside = ProcessTarget::parse(
                args.beside.clone().filter(|value| value != "self"),
                "run.beside",
                "page",
            )?;
            let mode = args.mode.map(Into::into).unwrap_or(PlacementMode::Auto);
            let command = if placement_override {
                AgentCommand::RunWithPlacementOverride {
                    anchor,
                    command,
                    direction,
                    focus: args.focus,
                    beside,
                    mode,
                    terminal,
                    done_marker: None,
                }
            } else {
                AgentCommand::Run {
                    anchor,
                    command,
                    direction,
                    focus: args.focus,
                    beside,
                    mode,
                    terminal,
                    done_marker: None,
                }
            };
            Ok(command)
        });
        match command {
            Ok(command) => {
                if let Ok(request) = protocol_requests.get(entity) {
                    commands
                        .entity(entity)
                        .insert(McpExecution::new(run_blocking(
                            command,
                            request.run_block_timeout(),
                        )));
                } else {
                    commands.entity(entity).insert(ToolCommand(Ok(command)));
                }
            }
            Err(message) => {
                commands.entity(entity).insert(ToolCommand(Err(message)));
            }
        }
    }
}

struct ProcessTarget;

impl ProcessTarget {
    fn parse(
        value: Option<String>,
        field: &str,
        target: &str,
    ) -> Result<Option<ProcessId>, String> {
        let Some(value) = value.filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        value
            .parse()
            .map(Some)
            .map_err(|_| format!("{field} is not a valid {target} id: {value}"))
    }
}

fn create_worktree(
    mut commands: Commands,
    requests: Query<
        (Entity, &Name, Option<&ProcessAnchor>, &CreateWorktreeArgs),
        Added<CreateWorktreeArgs>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).map(|anchor| {
            if let Some(branch) = args.branch.clone().and_then(Trimmed::into_option) {
                AgentCommand::CreateWorktreeOnBranch {
                    anchor,
                    branch,
                    project: None,
                }
            } else {
                AgentCommand::PrepareWorktree {
                    anchor,
                    path: args.path.clone().and_then(Trimmed::into_option),
                    task: args.task.clone().and_then(Trimmed::into_option),
                    create: args.create,
                }
            }
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

struct Trimmed;

impl Trimmed {
    fn into_option(value: String) -> Option<String> {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }
}

fn request_user_choice(
    mut commands: Commands,
    requests: Query<
        (
            Entity,
            &Name,
            Option<&ProcessAnchor>,
            &RequestUserChoiceArgs,
        ),
        Added<RequestUserChoiceArgs>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| {
            let question = Trimmed::into_option(args.question.clone())
                .ok_or("request_user_choice.question is empty")?;
            let options = args
                .options
                .iter()
                .cloned()
                .map(Trimmed::into_option)
                .collect::<Option<Vec<_>>>()
                .ok_or("request_user_choice options must be non-empty strings")?;
            if !(2..=9).contains(&options.len()) {
                return Err("request_user_choice requires 2 to 9 options".to_string());
            }
            Ok(AgentCommand::RequestUserChoice {
                anchor,
                question,
                options,
            })
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn select_project(
    mut commands: Commands,
    requests: Query<
        (Entity, &Name, Option<&ProcessAnchor>, &SelectProjectArgs),
        Added<SelectProjectArgs>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).map(|anchor| {
            match args.path.clone().and_then(Trimmed::into_option) {
                Some(path) => AgentCommand::ChooseWorkspaceAtPath { anchor, path },
                None => AgentCommand::ChooseWorkspace { anchor },
            }
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn read_terminal(
    mut commands: Commands,
    requests: Query<(Entity, &ReadTerminalArgs), AddedTool<ReadTerminalArgs>>,
) {
    for (entity, args) in &requests {
        let query = args
            .terminal
            .parse()
            .map(|process_id| AgentQuery::ReadProcessOutput { process_id })
            .map_err(|_| "read_terminal.terminal must be a valid terminal id".to_string());
        commands.entity(entity).insert(ToolQuery(query));
    }
}

async fn agent_working_directory(anchor: Option<ProcessId>) -> Result<PathBuf, String> {
    let Some(anchor) = anchor else {
        return std::env::current_dir()
            .and_then(|path| path.canonicalize())
            .map_err(|error| format!("cannot resolve current directory: {error}"));
    };
    let connection = ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    let request_id = AgentRequestId::new();
    connection
        .send(&ClientMessage::AgentQuery {
            request_id,
            query: AgentQuery::WorkingDirectory { anchor },
        })
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
            ServiceMessage::AgentWorkingDirectoryResult {
                request_id: received,
                result,
            } if received == request_id => {
                return PathBuf::from(result?)
                    .canonicalize()
                    .map_err(|error| format!("cannot resolve agent working directory: {error}"));
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
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
    anchor: Option<ProcessId>,
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
    process_id: &str,
    exit: Option<i32>,
    output: &str,
    timed_out: bool,
    run_block_timeout: Duration,
) -> Value {
    let mut text = format!("terminal: {process_id}\n");
    match exit {
        Some(code) => text.push_str(&format!("exit: {code}\n")),
        None if timed_out => text.push_str(&format!(
            "note: still running after {}s; call read_terminal({process_id}) to read more\n",
            run_block_timeout.as_secs()
        )),
        None => {}
    }
    text.push_str("output:\n");
    text.push_str(output);
    json!({ "content": [{"type": "text", "text": text}] })
}

fn run_completion_exit(
    result: Result<AgentRunCompletion, String>,
    token: &str,
    process_id: ProcessId,
    allow_missing_process: bool,
) -> Result<Option<i32>, String> {
    match result {
        Ok(AgentRunCompletion {
            token: Some(done_token),
            exit: Some(exit),
        }) if done_token == token => Ok(Some(exit)),
        Ok(_) => Ok(None),
        Err(message)
            if allow_missing_process && message == format!("process not found: {process_id}") =>
        {
            Ok(None)
        }
        Err(message) => Err(message),
    }
}

async fn run_blocking(run: AgentCommand, run_block_timeout: Duration) -> Result<Value, String> {
    let connection = ServiceConnection::connect()
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

    let process_id = loop {
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
            } if received == request_id => match result {
                vmux_api::protocol::AgentCommandResult::Text(process_id) => break process_id,
                vmux_api::protocol::AgentCommandResult::Error(message) => return Err(message),
                other => return Err(format!("run: unexpected result: {other:?}")),
            },
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    };

    let process_id = process_id
        .parse::<ProcessId>()
        .map_err(|_| format!("run: service returned an invalid terminal id: {process_id}"))?;
    let start = Instant::now();
    let baseline_text = read_full_text(&connection, process_id).await;
    let deadline = start + run_block_timeout;
    let materialize_deadline = start + RUN_PROCESS_MATERIALIZE_TIMEOUT;
    let mut process_materialized = false;
    loop {
        let result = run_completion(&connection, process_id).await?;
        let materialized_now = result.is_ok();
        let allow_missing_process = !process_materialized && Instant::now() < materialize_deadline;
        let exit = run_completion_exit(result, &token, process_id, allow_missing_process)?;
        process_materialized |= materialized_now;
        if let Some(exit) = exit {
            let final_text = read_full_text(&connection, process_id).await;
            let output = output_since(&baseline_text, &final_text);
            return Ok(run_result(
                &process_id.to_string(),
                Some(exit),
                &output,
                false,
                run_block_timeout,
            ));
        }
        if Instant::now() >= deadline {
            let final_text = read_full_text(&connection, process_id).await;
            let output = output_since(&baseline_text, &final_text);
            return Ok(run_result(
                &process_id.to_string(),
                None,
                &output,
                true,
                run_block_timeout,
            ));
        }
        tokio::time::sleep(RUN_POLL_INTERVAL).await;
    }
}

async fn run_completion(
    connection: &ServiceConnection,
    process_id: ProcessId,
) -> Result<Result<AgentRunCompletion, String>, String> {
    let request_id = AgentRequestId::new();
    connection
        .send(&ClientMessage::AgentQuery {
            request_id,
            query: AgentQuery::ProcessRunCompletion { process_id },
        })
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
            ServiceMessage::ProcessRunCompletionResult {
                request_id: received,
                result,
            } if received == request_id => return Ok(result),
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}

async fn read_full_text(connection: &ServiceConnection, process_id: ProcessId) -> String {
    let request_id = AgentRequestId::new();
    if connection
        .send(&ClientMessage::AgentQuery {
            request_id,
            query: AgentQuery::ReadProcessTranscript { process_id },
        })
        .await
        .is_err()
    {
        return String::new();
    }
    loop {
        let Ok(Some(message)) = connection.recv().await else {
            return String::new();
        };
        match message {
            ServiceMessage::ProcessTranscriptResult {
                request_id: received,
                result,
            } if received == request_id => return result.unwrap_or_default(),
            ServiceMessage::Error { .. } => return String::new(),
            _ => {}
        }
    }
}

async fn run_agent_command(
    command: AgentCommand,
    anchor: Option<ProcessId>,
) -> Result<Value, String> {
    let request_id = AgentRequestId::new();
    let connection = ServiceConnection::connect()
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
                return vmux_mcp::protocol::command_result_to_mcp_response(result);
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}
