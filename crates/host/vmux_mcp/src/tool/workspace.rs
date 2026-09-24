use super::{
    DispatchTarget, NextToolOrder, RegisterTools, ToolCall, ToolCalls, ToolDispatchResult,
    ToolDispatchSet, ToolManifest, ToolRequestSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{
    AgentCommand, AgentPaneDirection, AgentQuery, PlacementMode, ProcessId,
};

pub(super) struct WorkspaceToolPlugin;

impl Plugin for WorkspaceToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(RegisterTools))
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

fn register(mut commands: Commands, mut next_order: ResMut<NextToolOrder>) {
    ToolManifest::<WorkspaceTool>::from_ron(include_str!("workspace.ron"))
        .spawn(&mut commands, &mut next_order);
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

fn resume_in_acp(mut commands: Commands, calls: ToolCalls<WorkspaceTool>) {
    for (request, call, _) in calls.matching(WorkspaceTool::ResumeInAcp) {
        let result = call
            .require_anchor()
            .map(|anchor| DispatchTarget::Command(AgentCommand::ResumeInAcp { anchor }));
        commands.entity(request).insert(ToolDispatchResult(result));
    }
}

fn parse(mut commands: Commands, calls: ToolCalls<WorkspaceTool>) {
    for (request, call, tool) in calls.iter() {
        match tool {
            WorkspaceTool::ResumeInAcp => {}
            WorkspaceTool::OpenPage => call.parse_into::<OpenPageArgs>(request, &mut commands),
            WorkspaceTool::OpenFile => call.parse_into::<OpenFileArgs>(request, &mut commands),
            WorkspaceTool::Run => call.parse_into::<RunArgs>(request, &mut commands),
            WorkspaceTool::RequestUserChoice => {
                call.parse_into::<RequestUserChoiceArgs>(request, &mut commands)
            }
            WorkspaceTool::SelectProject => {
                call.parse_into::<SelectProjectArgs>(request, &mut commands)
            }
            WorkspaceTool::CreateWorktree => {
                call.parse_into::<CreateWorktreeArgs>(request, &mut commands)
            }
            WorkspaceTool::ReadTerminal => {
                call.parse_into::<ReadTerminalArgs>(request, &mut commands)
            }
        }
    }
}

fn open_page(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &OpenPageArgs), Added<OpenPageArgs>>,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().and_then(|anchor| {
            if args.url.trim().is_empty() {
                return Err("open_page.url is empty".to_string());
            }
            Ok(DispatchTarget::Command(AgentCommand::OpenBeside {
                anchor,
                direction: args.direction.map(Into::into),
                url: args.url.clone(),
                focus: args.focus,
            }))
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn open_file(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &OpenFileArgs), Added<OpenFileArgs>>,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().and_then(|anchor| {
            let path = args.path.trim();
            if path.is_empty() {
                return Err("open_file.path is empty".to_string());
            }
            let url = if path.starts_with("file:") {
                path.to_string()
            } else {
                format!("file://{path}")
            };
            Ok(DispatchTarget::Command(AgentCommand::OpenBeside {
                anchor,
                direction: args.direction.map(Into::into),
                url,
                focus: args.focus,
            }))
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn run(mut commands: Commands, requests: Query<(Entity, &ToolCall, &RunArgs), Added<RunArgs>>) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().and_then(|anchor| {
            let placement_override =
                args.mode.is_some() || args.direction.is_some() || args.beside.is_some();
            let mut command = args.command.clone();
            if command.trim().is_empty() {
                return Err("run.command is empty".to_string());
            }
            if let Some(interpreter) = args.shell.as_ref().filter(|value| !value.trim().is_empty())
            {
                command = crate::host_quote::HostQuote::handing_to(
                    &call.host_shell,
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
            Ok(DispatchTarget::Command(command))
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
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
    requests: Query<(Entity, &ToolCall, &CreateWorktreeArgs), Added<CreateWorktreeArgs>>,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().map(|anchor| {
            let command = if let Some(branch) = args.branch.clone().and_then(Trimmed::into_option) {
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
            };
            DispatchTarget::Command(command)
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
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
    requests: Query<(Entity, &ToolCall, &RequestUserChoiceArgs), Added<RequestUserChoiceArgs>>,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().and_then(|anchor| {
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
            Ok(DispatchTarget::Command(AgentCommand::RequestUserChoice {
                anchor,
                question,
                options,
            }))
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn select_project(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &SelectProjectArgs), Added<SelectProjectArgs>>,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().map(|anchor| {
            let command = match args.path.clone().and_then(Trimmed::into_option) {
                Some(path) => AgentCommand::ChooseWorkspaceAtPath { anchor, path },
                None => AgentCommand::ChooseWorkspace { anchor },
            };
            DispatchTarget::Command(command)
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn read_terminal(
    mut commands: Commands,
    requests: Query<(Entity, &ReadTerminalArgs), (With<ToolCall>, Added<ReadTerminalArgs>)>,
) {
    for (entity, args) in &requests {
        let target = args
            .terminal
            .parse()
            .map(|process_id| DispatchTarget::Query(AgentQuery::ReadTerminal { process_id }))
            .map_err(|_| "read_terminal.terminal must be a valid terminal id".to_string());
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}
