use super::SharedAgentCommand;
use crate::ProcessId;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct AgentRequestId(pub [u8; 16]);

impl Default for AgentRequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRequestId {
    pub fn new() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentShellMode {
    NewTab,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentPaneDirection {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ManagedMcpTransport {
    Stdio,
    Http,
    Sse,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ManagedMcpServer {
    pub name: String,
    pub transport: ManagedMcpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<String>,
    pub url: Option<String>,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum PlacementMode {
    Auto,
    Split,
    Stack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum FileTouchKind {
    Read,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SimulatorButton {
    Home,
    Lock,
    Siri,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SimulatorAction {
    Tap {
        x: u32,
        y: u32,
    },
    Swipe {
        start_x: u32,
        start_y: u32,
        end_x: u32,
        end_y: u32,
        duration_ms: u32,
    },
    TypeText(String),
    Key(u8),
    Button(SimulatorButton),
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FileSearchMatch {
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentCommand {
    AppCommand {
        id: String,
        #[rkyv(attr(allow(dead_code)))]
        args_json: String,
    },
    NewTerminalTab {
        cwd: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    RunShell {
        command: String,
        cwd: String,
        mode: AgentShellMode,
    },
    BrowserNavigate {
        url: String,
        pane: Option<String>,
    },
    BrowserInstallExtension {
        source: String,
    },
    TerminalSend {
        text: String,
        terminal: Option<String>,
    },
    FocusPane {
        pane: String,
    },
    RenameProfile {
        name: String,
    },
    UpdateSettings {
        path: String,
        value_json: String,
    },
    UpdateLayout {
        layout: crate::protocol::layout::LayoutSnapshot,
    },
    BrowserGoBack {
        pane: Option<String>,
    },
    BrowserGoForward {
        pane: Option<String>,
    },
    BrowserHistorySearch {
        query: String,
        limit: u32,
    },
    OpenInNewStack {
        url: String,
    },
    SpaceCommand {
        command: String,
        space_id: Option<String>,
        name: Option<String>,
    },
    OpenBeside {
        anchor: ProcessId,
        direction: Option<AgentPaneDirection>,
        url: String,
        focus: bool,
    },
    Run {
        anchor: ProcessId,
        command: String,
        direction: AgentPaneDirection,
        focus: bool,
        beside: Option<ProcessId>,
        mode: PlacementMode,
        terminal: Option<ProcessId>,
        done_marker: Option<String>,
    },
    Notify {
        title: Option<String>,
        body: Option<String>,
    },
    FileTouched {
        anchor: ProcessId,
        path: String,
        line: Option<u32>,
        col: Option<u32>,
        end_col: Option<u32>,
        kind: FileTouchKind,
    },
    CreateWorktree {
        anchor: ProcessId,
    },
    TurnEnded {
        anchor: ProcessId,
    },
    RunWithPlacementOverride {
        anchor: ProcessId,
        command: String,
        direction: AgentPaneDirection,
        focus: bool,
        beside: Option<ProcessId>,
        mode: PlacementMode,
        terminal: Option<ProcessId>,
        done_marker: Option<String>,
    },
    ResumeInAcp {
        anchor: ProcessId,
    },
    ChooseWorkspace {
        anchor: ProcessId,
    },
    CreateWorktreeOnBranch {
        anchor: ProcessId,
        branch: String,
        project: Option<String>,
    },
    BookmarkCommand {
        command: String,
        uuid: Option<String>,
        name: Option<String>,
        url: Option<String>,
        title: Option<String>,
        favicon_url: Option<String>,
    },
    RequestUserChoice {
        anchor: ProcessId,
        question: String,
        options: Vec<String>,
    },
    ChooseWorkspaceAtPath {
        anchor: ProcessId,
        path: String,
    },
    PrepareWorktree {
        anchor: ProcessId,
        path: Option<String>,
        task: Option<String>,
        create: bool,
    },
    FileSearch {
        anchor: ProcessId,
        root: String,
        query: String,
        matches: Vec<FileSearchMatch>,
    },
    SetConversationTitle {
        anchor: ProcessId,
        title: String,
    },
    WriteKnowledge {
        anchor: ProcessId,
        path: Option<String>,
        title: String,
        content: String,
    },
    SearchKnowledge {
        anchor: ProcessId,
        query: String,
        limit: u16,
    },
    ReadKnowledge {
        anchor: ProcessId,
        path: String,
        line: u32,
        limit: u32,
    },
    Shared(SharedAgentCommand),
}

pub const AGENT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub const RECORD_STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub const AGENT_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub const BROWSER_NAVIGATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub const AGENT_TOOL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentCommandResult {
    Ok,
    Text(String),
    Layout(crate::protocol::layout::LayoutSnapshot),
    Error(String),
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum ApprovalDecision {
    Allow,
    #[default]
    Deny,
    AllowAlways,
}

impl ApprovalDecision {
    pub const OFFERED: [Self; 3] = [Self::Allow, Self::AllowAlways, Self::Deny];

    pub fn for_index(index: usize) -> Option<Self> {
        Self::OFFERED.get(index).copied()
    }
}

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentRunStatus {
    Streaming,
    Idle,
    Interrupted,
    Errored(String),
}

pub fn validate_agent_command(command: &AgentCommand) -> Result<(), &'static str> {
    match command {
        AgentCommand::AppCommand { id, .. } if id.trim().is_empty() => {
            Err("app_command.id is empty")
        }
        AgentCommand::RunShell { command, .. } if command.trim().is_empty() => {
            Err("run_shell.command is empty")
        }
        AgentCommand::BrowserNavigate { url, .. } if url.trim().is_empty() => {
            Err("browser_navigate.url is empty")
        }
        AgentCommand::BrowserInstallExtension { source } if source.trim().is_empty() => {
            Err("browser_install_extension.source is empty")
        }
        AgentCommand::TerminalSend { text, .. } if text.is_empty() => {
            Err("terminal_send.text is empty")
        }
        AgentCommand::FocusPane { pane } if pane.trim().is_empty() => {
            Err("focus_pane.pane is empty")
        }
        AgentCommand::RenameProfile { name } if name.trim().is_empty() => {
            Err("rename_profile.name is empty")
        }
        AgentCommand::UpdateSettings { path, .. } if path.trim().is_empty() => {
            Err("update_settings.path is empty")
        }
        AgentCommand::BrowserHistorySearch { query, .. } if query.trim().is_empty() => {
            Err("browser_history_search.query is empty")
        }
        AgentCommand::OpenInNewStack { url, .. } if url.trim().is_empty() => {
            Err("open_in_new_stack.url is empty")
        }
        AgentCommand::SpaceCommand { command, .. } if command.trim().is_empty() => {
            Err("space_command.command is empty")
        }
        AgentCommand::BookmarkCommand { command, .. } if command.trim().is_empty() => {
            Err("bookmark_command.command is empty")
        }
        AgentCommand::OpenBeside { url, .. } if url.trim().is_empty() => {
            Err("open_beside_me.url is empty")
        }
        AgentCommand::Run { command, .. }
        | AgentCommand::RunWithPlacementOverride { command, .. }
            if command.trim().is_empty() =>
        {
            Err("run.command is empty")
        }
        AgentCommand::FileTouched { path, .. } if path.trim().is_empty() => {
            Err("file_touched.path is empty")
        }
        AgentCommand::CreateWorktreeOnBranch { branch, .. } if branch.trim().is_empty() => {
            Err("create_worktree.branch is empty")
        }
        AgentCommand::RequestUserChoice {
            question, options, ..
        } if question.trim().is_empty()
            || options.len() < 2
            || options.len() > 9
            || options.iter().any(|option| option.trim().is_empty()) =>
        {
            Err("request_user_choice requires a question and 2 to 9 non-empty options")
        }
        AgentCommand::ChooseWorkspaceAtPath { path, .. } if path.trim().is_empty() => {
            Err("select_project.path is empty")
        }
        AgentCommand::WriteKnowledge {
            path,
            title,
            content,
            ..
        } if path.as_ref().is_some_and(|path| path.trim().is_empty())
            || title.trim().is_empty()
            || content.trim().is_empty() =>
        {
            Err("write_knowledge requires a non-empty title and content")
        }
        AgentCommand::SearchKnowledge { query, limit, .. }
            if query.trim().is_empty() || *limit == 0 || *limit > 100 =>
        {
            Err("search_knowledge requires a query and limit between 1 and 100")
        }
        AgentCommand::ReadKnowledge { path, limit, .. }
            if path.trim().is_empty() || *limit == 0 || *limit > 2_000 =>
        {
            Err("read_knowledge requires a path and limit between 1 and 2000")
        }
        AgentCommand::Shared(SharedAgentCommand::NewAgentChat { prompt, .. })
            if prompt.trim().is_empty() =>
        {
            Err("new_agent_chat.prompt is empty")
        }
        _ => Ok(()),
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentAttachment {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
}

pub const PRIVATE_CONTEXT_PREFIX: &str = "<vmux_handoff_context>";
pub const PRIVATE_CONTEXT_PROMPT_MARKER: &str = "\n\nCurrent user prompt:\n";
const PRIVATE_CONTEXT_LENGTH_PREFIX: &str = "Context bytes: ";
const PRIVATE_CONTEXT_CLOSING_TAG: &str = "\n</vmux_handoff_context>";

pub fn compose_agent_prompt(display_text: &str, context: Option<&str>) -> String {
    match context {
        Some(context) => format!(
            "{PRIVATE_CONTEXT_PREFIX}\n{PRIVATE_CONTEXT_LENGTH_PREFIX}{}\n{context}{PRIVATE_CONTEXT_CLOSING_TAG}{PRIVATE_CONTEXT_PROMPT_MARKER}{display_text}",
            context.len()
        ),
        None => display_text.to_string(),
    }
}

pub fn extract_display_prompt(prompt: &str) -> Option<&str> {
    split_private_context_prompt(prompt).map(|(_, display)| display)
}

pub fn split_private_context_prompt(prompt: &str) -> Option<(&str, &str)> {
    split_length_delimited_private_context(prompt).or_else(|| {
        let body = private_context_body(prompt)?;
        let separator = format!("{PRIVATE_CONTEXT_CLOSING_TAG}{PRIVATE_CONTEXT_PROMPT_MARKER}");
        body.rsplit_once(&separator)
    })
}

pub fn has_private_context_envelope(prompt: &str) -> bool {
    private_context_body(prompt).is_some_and(|body| body.contains(PRIVATE_CONTEXT_CLOSING_TAG))
}

fn private_context_body(prompt: &str) -> Option<&str> {
    prompt
        .find(PRIVATE_CONTEXT_PREFIX)
        .and_then(|start| prompt.get(start + PRIVATE_CONTEXT_PREFIX.len()..))?
        .strip_prefix('\n')
}

fn split_length_delimited_private_context(prompt: &str) -> Option<(&str, &str)> {
    let body = private_context_body(prompt)?;
    let (length, body) = body.split_once('\n')?;
    let context_len = length
        .strip_prefix(PRIVATE_CONTEXT_LENGTH_PREFIX)?
        .parse::<usize>()
        .ok()?;
    let context = body.get(..context_len)?;
    let display = body
        .get(context_len..)?
        .strip_prefix(PRIVATE_CONTEXT_CLOSING_TAG)?
        .strip_prefix(PRIVATE_CONTEXT_PROMPT_MARKER)?;
    Some((context, display))
}
