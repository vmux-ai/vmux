use super::{DispatchTarget, ToolCall, ToolManifest};
use bevy_app::{App, Plugin};
use serde::Deserialize;
use vmux_client::protocol::{
    AgentCommand, AgentPaneDirection, AgentQuery, PlacementMode, ProcessId,
};

pub(super) struct WorkspaceToolsPlugin;

impl Plugin for WorkspaceToolsPlugin {
    fn build(&self, app: &mut App) {
        let mut tools = ToolManifest::from_ron(include_str!("workspace.ron"));
        tools.local(app, "open_page", open_page);
        tools.local(app, "open_file", open_file);
        tools.local(app, "resume_in_acp", resume_in_acp);
        tools.local(app, "run", run);
        tools.local(app, "request_user_choice", request_user_choice);
        tools.local(app, "select_project", select_project);
        tools.local(app, "create_worktree", create_worktree);
        tools.local(app, "read_terminal", read_terminal);
        tools.finish();
    }
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
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

#[derive(Deserialize)]
struct OpenPageArgs {
    url: Option<String>,
    direction: Option<PaneDirection>,
    #[serde(default)]
    focus: bool,
}

#[derive(Deserialize)]
struct OpenFileArgs {
    path: Option<String>,
    direction: Option<PaneDirection>,
    #[serde(default)]
    focus: bool,
}

#[derive(Deserialize)]
struct RunArgs {
    command: Option<String>,
    shell: Option<String>,
    direction: Option<PaneDirection>,
    #[serde(default)]
    focus: bool,
    terminal: Option<String>,
    beside: Option<String>,
    mode: Option<RunMode>,
}

#[derive(Deserialize)]
struct CreateWorktreeArgs {
    branch: Option<String>,
    path: Option<String>,
    task: Option<String>,
    #[serde(default)]
    create: bool,
}

#[derive(Deserialize)]
struct RequestUserChoiceArgs {
    question: Option<String>,
    options: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct SelectProjectArgs {
    path: Option<String>,
}

#[derive(Deserialize)]
struct ReadTerminalArgs {
    terminal: Option<String>,
}

pub(super) fn resume_in_acp(call: &ToolCall) -> Result<DispatchTarget, String> {
    let anchor = call.require_anchor("resume_in_acp")?;
    Ok(DispatchTarget::Command(AgentCommand::ResumeInAcp {
        anchor,
    }))
}

pub(super) fn open_page(call: &ToolCall) -> Result<DispatchTarget, String> {
    let anchor = call.require_anchor("open_page")?;
    let args: OpenPageArgs = call.parse("open_page")?;
    let url = args.url.unwrap_or_default();
    if url.trim().is_empty() {
        return Err("open_page.url is empty".to_string());
    }
    Ok(DispatchTarget::Command(AgentCommand::OpenBeside {
        anchor,
        direction: args.direction.map(Into::into),
        url,
        focus: args.focus,
    }))
}

pub(super) fn open_file(call: &ToolCall) -> Result<DispatchTarget, String> {
    let anchor = call.require_anchor("open_file")?;
    let args: OpenFileArgs = call.parse("open_file")?;
    let path = args.path.unwrap_or_default().trim().to_string();
    if path.is_empty() {
        return Err("open_file.path is empty".to_string());
    }
    let url = if path.starts_with("file:") {
        path
    } else {
        format!("file://{path}")
    };
    Ok(DispatchTarget::Command(AgentCommand::OpenBeside {
        anchor,
        direction: args.direction.map(Into::into),
        url,
        focus: args.focus,
    }))
}

pub(super) fn run(call: &ToolCall) -> Result<DispatchTarget, String> {
    let anchor = call.require_anchor("run")?;
    let placement_override = ["mode", "direction", "beside"].iter().any(|key| {
        call.arguments
            .get(*key)
            .is_some_and(|value| !value.is_null())
    });
    let args: RunArgs = call.parse("run")?;
    let mut command = args.command.unwrap_or_default();
    if command.trim().is_empty() {
        return Err("run.command is empty".to_string());
    }
    if let Some(interpreter) = args.shell.filter(|value| !value.trim().is_empty()) {
        command =
            crate::host_quote::HostQuote::handing_to(&call.host_shell, &interpreter, &command)?;
    }
    let direction = args
        .direction
        .map(Into::into)
        .unwrap_or(AgentPaneDirection::Right);
    let terminal = ProcessTarget::parse(args.terminal, "run.terminal", "terminal")?;
    let beside = ProcessTarget::parse(
        args.beside.filter(|value| value != "self"),
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
    Ok(DispatchTarget::Command(command))
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

pub(super) fn create_worktree(call: &ToolCall) -> Result<DispatchTarget, String> {
    let anchor = call.require_anchor("create_worktree")?;
    let args: CreateWorktreeArgs = call.parse("create_worktree")?;
    if let Some(branch) = args.branch.and_then(Trimmed::into_option) {
        return Ok(DispatchTarget::Command(
            AgentCommand::CreateWorktreeOnBranch {
                anchor,
                branch,
                project: None,
            },
        ));
    }
    Ok(DispatchTarget::Command(AgentCommand::PrepareWorktree {
        anchor,
        path: args.path.and_then(Trimmed::into_option),
        task: args.task.and_then(Trimmed::into_option),
        create: args.create,
    }))
}

struct Trimmed;

impl Trimmed {
    fn into_option(value: String) -> Option<String> {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }
}

pub(super) fn request_user_choice(call: &ToolCall) -> Result<DispatchTarget, String> {
    let anchor = call.require_anchor("request_user_choice")?;
    let args: RequestUserChoiceArgs = call.parse("request_user_choice")?;
    let question = args
        .question
        .and_then(Trimmed::into_option)
        .ok_or("request_user_choice.question is empty")?;
    let options = args
        .options
        .ok_or("request_user_choice.options must be an array")?
        .into_iter()
        .map(Trimmed::into_option)
        .collect::<Option<Vec<_>>>()
        .ok_or("request_user_choice options must be non-empty strings")?;
    if !(2..=9).contains(&options.len()) {
        return Err("request_user_choice requires 2 to 9 options".to_string());
    }
    Ok(DispatchTarget::Command(AgentCommand::RequestUserChoice {
        anchor,
        question,
        options,
    }))
}

pub(super) fn select_project(call: &ToolCall) -> Result<DispatchTarget, String> {
    let anchor = call.require_anchor("select_project")?;
    let args: SelectProjectArgs = call.parse("select_project")?;
    let Some(path) = args.path.and_then(Trimmed::into_option) else {
        return Ok(DispatchTarget::Command(AgentCommand::ChooseWorkspace {
            anchor,
        }));
    };
    Ok(DispatchTarget::Command(
        AgentCommand::ChooseWorkspaceAtPath { anchor, path },
    ))
}

pub(super) fn read_terminal(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: ReadTerminalArgs = call.parse("read_terminal")?;
    let process_id = args
        .terminal
        .unwrap_or_default()
        .parse()
        .map_err(|_| "read_terminal.terminal must be a valid terminal id".to_string())?;
    Ok(DispatchTarget::Query(AgentQuery::ReadTerminal {
        process_id,
    }))
}
