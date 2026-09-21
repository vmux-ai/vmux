use super::{DispatchTarget, ToolAvailability, ToolCall, ToolDefinition, ToolRegistration};
use bevy_app::{App, Plugin};
use serde::Deserialize;
use vmux_client::protocol::{
    AgentCommand, AgentPaneDirection, AgentQuery, PlacementMode, ProcessId,
};

pub(super) struct WorkspaceToolsPlugin;

impl Plugin for WorkspaceToolsPlugin {
    fn build(&self, app: &mut App) {
        ToolRegistration::from_definition(open_page_definition()).local(app, open_page);
        ToolRegistration::from_definition(open_file_definition()).local(app, open_file);
        ToolRegistration::from_definition(resume_in_acp_definition())
            .availability(ToolAvailability::OutsideAcpSession)
            .local(app, resume_in_acp);
        ToolRegistration::from_definition(run_definition())
            .availability(ToolAvailability::WithoutAcpTerminals)
            .shell_aware()
            .local(app, run);
        ToolRegistration::from_definition(request_user_choice_definition())
            .local(app, request_user_choice);
        ToolRegistration::from_definition(select_project_definition())
            .aliases(&["select_workspace", "choose_workspace"])
            .local(app, select_project);
        ToolRegistration::from_definition(create_worktree_definition()).local(app, create_worktree);
        ToolRegistration::from_definition(read_terminal_definition())
            .availability(ToolAvailability::WithoutAcpTerminals)
            .local(app, read_terminal);
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

pub(super) fn open_page_definition() -> ToolDefinition {
    ToolDefinition {
        name: "open_page".into(),
        description: "Open a page using vmux auto placement. Omit `direction` so vmux reuses \
the existing matching bucket first (terminal pages with terminals, browser pages with browsers) \
and otherwise spirals off the latest non-agent pane. url uses the same rules as browser_navigate \
(vmux://terminal/ opens a terminal; anything else loads as a browser). direction is an override \
for a forced adjacent open: right|left|top|bottom. focus defaults false."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["url"],
            "additionalProperties": false,
            "properties": {
                "direction": {"enum": ["right", "left", "top", "bottom"]},
                "url": {"type": "string"},
                "focus": {"type": "boolean"}
            }
        }),
    }
}

pub(super) fn open_file_definition() -> ToolDefinition {
    ToolDefinition {
        name: "open_file".into(),
        description: "Open a local file (or directory) in the vmux editor using vmux auto \
placement. Omit `direction` so vmux focuses an already-open matching file first, then reuses \
the file pane bucket, and otherwise spirals off the latest non-agent pane. path is an absolute \
filesystem path, e.g. /Users/me/project/src/main.rs. Files render with syntax highlighting; \
directories show a listing. direction is an override for a forced adjacent open: \
right|left|top|bottom. focus defaults false. The path must be inside the selected project; call \
select_project first to request access elsewhere."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["path"],
            "additionalProperties": false,
            "properties": {
                "path": {"type": "string"},
                "direction": {"enum": ["right", "left", "top", "bottom"]},
                "focus": {"type": "boolean"}
            }
        }),
    }
}

pub(super) fn run_definition() -> ToolDefinition {
    ToolDefinition {
        name: "run".into(),
        description:
            "Run a shell command in a visible terminal pane the user can watch live and take over. \
Blocks until the command finishes and returns its full output plus the exit code \
(`terminal: <id>`, `exit: <code>`, `output: ...`). If it reaches the configured wait limit, returns \
the output so far with a note to call read_terminal for the rest. \
\
PLACEMENT — by DEFAULT you don't need to think about this: a bare `run` reuses ONE persistent terminal \
beside you — the SAME shell across calls, so its working directory and environment persist. Do NOT `cd` \
into your project on every run; the shell stays where it was. The first `run` opens it; later ones run \
in that same shell. Rule of thumb: don't open a new pane unless you actually need one. \
Placement overrides are disabled by default: omit `mode`, `direction`, and `beside`. If vmux rejects \
them, retry the bare run. Users can enable overrides with `agent.allow_run_placement_override`. \
When enabled, override only when you mean to: \
- `mode`: `auto` (default, reuse your one persistent shell) | `split` (force a NEW pane) | `stack` \
(force a new stacked terminal in the anchor's pane). \
- `beside`: anchor to a specific page — a terminal id a previous run returned, or \"self\" for your own \
pane. With `beside` set, `stack` tabs into that page's pane and `split` splits off it. \
- `direction`: only for `split`; Omit `direction` in auto mode so vmux keeps terminal runs in the \
terminal bucket and spirals new panes predictably. \
- `terminal: <id>`: instead of opening anything, run IN that existing terminal (best for dependent / \
sequential steps that share one shell, in order). \
\
`focus` (default false = keep focus on your own pane) applies when opening a new terminal. The command \
is typed into an interactive shell, so the terminal stays usable afterwards. \
\
`shell`: hand the command to a named interpreter instead of writing it in the user's shell — \
`bash`, `sh`, `python3`, `node`, `ruby`, anything on PATH. Name the program ALONE: vmux adds the \
flag that makes it read a script (`-c`, `-e`, `eval`) and quotes the script for the user's shell, \
so `command` is the script itself, newlines and all, and never carries `-c` or `-e` of its own. \
Omit `shell` to write in the user's own shell."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["command"],
            "additionalProperties": false,
            "properties": {
                "command": {"type": "string"},
                "shell": {"type": "string"},
                "terminal": {"type": "string"},
                "beside": {"type": "string"},
                "mode": {"enum": ["auto", "split", "stack"]},
                "direction": {"enum": ["right", "left", "top", "bottom"]},
                "focus": {"type": "boolean"}
            }
        }),
    }
}

pub(super) fn create_worktree_definition() -> ToolDefinition {
    ToolDefinition {
        name: "create_worktree".into(),
        description: "Call immediately before the first edit, write, test, build, or other project mutation, after a Git project is selected. Never call for requests that only read, show, search, or explain existing files. vmux reuses the current linked worktree, accepts a known existing worktree path, automatically uses a single unambiguous existing worktree, or creates a managed worktree when none exists. If multiple existing worktrees are returned as ambiguous, ask the user with request_user_choice to choose an existing path or Create new worktree; call again with path or create=true. Never run git worktree add manually. Returns the absolute worktree path."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "path": {"type": "string"},
                "branch": {"type": "string"},
                "task": {"type": "string"},
                "create": {"type": "boolean"}
            }
        }),
    }
}

pub(super) fn select_project_definition() -> ToolDefinition {
    ToolDefinition {
        name: "select_project".into(),
        description: "Select a project before accessing its files or running project commands. Pass a known path or omit it to open the native project picker rooted at ~/.vmux/projects. Paths inside ~/.vmux/projects are selected immediately. Paths outside it require explicit user approval in the native picker. For a new project, first use request_user_choice to offer a concrete suggested location and Choose existing project; do not ask the user to invent a folder. Use ~/.vmux/projects/<remote-host>/<organization>/<repository> when a remote is known and ~/.vmux/projects/local/<project> otherwise. When creation is selected, use run only to create the empty directory, then call this tool with that path. vmux offers Git initialization and uses the new project root directly without a linked worktree. For a previously existing Git project, call create_worktree immediately before the first mutation. The request returns immediately when user selection is needed: stop the current turn and do not call again while pending. Do not search the user's home directory. Do not call for general questions or self-contained terminal demonstrations."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "path": {"type": "string"}
            }
        }),
    }
}

pub(super) fn request_user_choice_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_user_choice".into(),
        description: "Show a native multiple-choice question in the agent conversation. For a new project without a selected project, use it to offer the concrete suggested ~/.vmux/projects path or Choose existing project. Also use it for other user-requested options and ambiguous worktree selection. Keep options concise and actionable. The user can choose with arrow keys, Ctrl+N/Ctrl+P, number keys, mouse, or Enter. The request returns immediately: stop the current turn; vmux resumes the same conversation with the selected option."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["question", "options"],
            "additionalProperties": false,
            "properties": {
                "question": {"type": "string"},
                "options": {
                    "type": "array",
                    "minItems": 2,
                    "maxItems": 9,
                    "items": {"type": "string"}
                }
            }
        }),
    }
}

pub(super) fn resume_in_acp_definition() -> ToolDefinition {
    ToolDefinition {
        name: "resume_in_acp".into(),
        description: "Continue the current CLI conversation in its ACP chat runtime. Replaces this CLI page in place while preserving the session id and working directory. Call only when the user asks to switch or continue in ACP."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        }),
    }
}

pub(super) fn read_terminal_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_terminal".into(),
        description:
            "Return the current visible scrollback text of a terminal (the same text the user sees). \
Pass `terminal` = a terminal id returned by run, or a terminal stack's process_id from read_layout."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["terminal"],
            "additionalProperties": false,
            "properties": {
                "terminal": {"type": "string"}
            }
        }),
    }
}
