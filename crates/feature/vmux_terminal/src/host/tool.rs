use bevy::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use vmux_api::protocol::{
    AgentRequest, AgentRequestId, AgentRunCompletion, ClientMessage, ProcessId, ServiceMessage,
};
use vmux_core::service::ServiceConnection;
use vmux_core::{HostShell, ProcessAnchor};
use vmux_mcp::protocol::{McpExecution, McpRequest};

use vmux_process::{AgentProcessRunCompletion, AgentReadProcessOutput, AgentReadProcessTranscript};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
};

use super::{AgentRun, AgentRunWithPlacementOverride, AgentTerminalSend, PlacementMode};
use vmux_layout::AgentPaneDirection;

const RUN_PROCESS_MATERIALIZE_TIMEOUT: Duration = Duration::from_secs(2);
const RUN_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub struct TerminalToolPlugin;

impl Plugin for TerminalToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("../feature.ron"),
            "default",
        ))
        .register_tool::<RunArgs>("run")
        .register_tool::<ReadTerminalArgs>("read_terminal")
        .register_tool::<TerminalSendArgs>("terminal_send")
        .add_systems(
            Update,
            (run, read_terminal, dispatch).in_set(ToolDispatchSet),
        );
    }
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
struct ReadTerminalArgs {
    terminal: String,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalSendArgs {
    text: String,
    terminal: Option<String>,
    enter: Option<bool>,
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
            Ok((
                AgentRun {
                    anchor,
                    command,
                    direction,
                    focus: args.focus,
                    beside,
                    mode,
                    terminal,
                    done_marker: None,
                },
                placement_override,
            ))
        });
        match command {
            Ok((run, placement_override)) => {
                if let Ok(request) = protocol_requests.get(entity) {
                    commands
                        .entity(entity)
                        .insert(McpExecution::new(run_blocking(
                            run,
                            placement_override,
                            request.run_block_timeout(),
                        )));
                } else {
                    let request = if placement_override {
                        AgentRequest::encode(&AgentRunWithPlacementOverride(run))
                    } else {
                        AgentRequest::encode(&run)
                    };
                    commands.entity(entity).insert(ToolCommand(request));
                }
            }
            Err(message) => {
                commands.entity(entity).insert(ToolCommand(Err(message)));
            }
        }
    }
}

fn read_terminal(
    mut commands: Commands,
    requests: Query<(Entity, &ReadTerminalArgs), AddedTool<ReadTerminalArgs>>,
) {
    for (entity, args) in &requests {
        let query = match args.terminal.parse() {
            Ok(process_id) => AgentRequest::encode(&AgentReadProcessOutput { process_id }),
            Err(_) => Err("read_terminal.terminal must be a valid terminal id".to_string()),
        };
        commands.entity(entity).insert(ToolQuery(query));
    }
}

fn dispatch(
    mut commands: Commands,
    requests: Query<(Entity, &TerminalSendArgs), AddedTool<TerminalSendArgs>>,
) {
    for (entity, args) in &requests {
        let text = if args.enter.unwrap_or(false) {
            format!("{}\r", args.text)
        } else {
            args.text.clone()
        };
        let command = if text.is_empty() {
            Err("terminal_send.text is empty".to_string())
        } else {
            AgentRequest::encode(&AgentTerminalSend {
                text,
                terminal: args.terminal.clone(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
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

async fn run_blocking(
    mut run: AgentRun,
    placement_override: bool,
    run_block_timeout: Duration,
) -> Result<Value, String> {
    let connection = ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    let request_id = AgentRequestId::new();
    let token = run_done_token(request_id);
    run.done_marker = Some(token.clone());
    let request = if placement_override {
        AgentRequest::encode(&AgentRunWithPlacementOverride(run))
    } else {
        AgentRequest::encode(&run)
    }?;
    connection
        .send(&ClientMessage::AgentRequest {
            request_id,
            anchor: None,
            request,
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
            query: AgentRequest::encode(&AgentProcessRunCompletion { process_id })?,
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
            ServiceMessage::AgentQueryResult(result) if result.request_id == request_id => {
                if result.is_error {
                    return Ok(Err(result.content));
                }
                return serde_json::from_str(&result.content)
                    .map(Ok)
                    .map_err(|error| format!("cannot decode process completion: {error}"));
            }
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
            query: match AgentRequest::encode(&AgentReadProcessTranscript { process_id }) {
                Ok(query) => query,
                Err(_) => return String::new(),
            },
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
            ServiceMessage::AgentQueryResult(result) if result.request_id == request_id => {
                return (!result.is_error)
                    .then_some(result.content)
                    .unwrap_or_default();
            }
            ServiceMessage::Error { .. } => return String::new(),
            _ => {}
        }
    }
}
