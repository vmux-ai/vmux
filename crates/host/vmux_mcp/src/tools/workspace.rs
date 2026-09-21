use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs, World};
use serde::Deserialize;
use vmux_client::protocol::{
    AgentCommand, AgentPaneDirection, AgentQuery, PlacementMode, ProcessId,
};

pub(super) struct WorkspaceToolsPlugin;

impl Plugin for WorkspaceToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Workspace))
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

#[derive(Component)]
struct OpenPage;

#[derive(Component)]
struct OpenFile;

#[derive(Component)]
struct ResumeInAcp;

#[derive(Component)]
struct Run;

#[derive(Component)]
struct RequestUserChoice;

#[derive(Component)]
struct SelectProject;

#[derive(Component)]
struct CreateWorktree;

#[derive(Component)]
struct ReadTerminal;

fn register(world: &mut World) {
    let mut tools = ToolManifest::from_ron(include_str!("workspace.ron"));
    tools.system(world, "open_page", OpenPage);
    tools.system(world, "open_file", OpenFile);
    tools.system(world, "resume_in_acp", ResumeInAcp);
    tools.system(world, "run", Run);
    tools.system(world, "request_user_choice", RequestUserChoice);
    tools.system(world, "select_project", SelectProject);
    tools.system(world, "create_worktree", CreateWorktree);
    tools.system(world, "read_terminal", ReadTerminal);
    tools.finish();
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

fn resume_in_acp(mut commands: Commands, calls: ToolCalls<ResumeInAcp>) {
    for (request, call, _) in calls.iter() {
        let result = call
            .require_anchor("resume_in_acp")
            .map(|anchor| DispatchTarget::Command(AgentCommand::ResumeInAcp { anchor }));
        call.finish_dispatch(request, &mut commands, result);
    }
}

fn open_page(mut commands: Commands, calls: ToolCalls<OpenPage>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    for (request, call, _) in calls.iter() {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn open_file(mut commands: Commands, calls: ToolCalls<OpenFile>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    for (request, call, _) in calls.iter() {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn run(mut commands: Commands, calls: ToolCalls<Run>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    for (request, call, _) in calls.iter() {
        call.finish_dispatch(request, &mut commands, target(call));
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

fn create_worktree(mut commands: Commands, calls: ToolCalls<CreateWorktree>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    for (request, call, _) in calls.iter() {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

struct Trimmed;

impl Trimmed {
    fn into_option(value: String) -> Option<String> {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }
}

fn request_user_choice(mut commands: Commands, calls: ToolCalls<RequestUserChoice>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    for (request, call, _) in calls.iter() {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn select_project(mut commands: Commands, calls: ToolCalls<SelectProject>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    for (request, call, _) in calls.iter() {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn read_terminal(mut commands: Commands, calls: ToolCalls<ReadTerminal>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    for (request, call, _) in calls.iter() {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}
