use super::SharedAgentCommand;
use crate::{ProcessId, json::JsonValue};

#[vmux_api::payload(Copy, Eq, Hash)]
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

#[vmux_api::payload(Copy, Eq)]
pub enum AgentShellMode {
    NewTab,
    Active,
}

#[vmux_api::payload(Copy, Eq)]
pub enum AgentPaneDirection {
    Top,
    Right,
    Bottom,
    Left,
}

#[vmux_api::payload(Copy, Eq)]
pub enum ManagedMcpTransport {
    Stdio,
    Http,
    Sse,
}

#[vmux_api::payload(Eq)]
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

#[vmux_api::payload(Copy, Eq)]
pub enum PlacementMode {
    Auto,
    Split,
    Stack,
}

#[vmux_api::payload(Copy, Eq)]
pub enum FileTouchKind {
    Read,
    Edit,
}

#[vmux_api::payload(Copy, Eq)]
pub enum SimulatorButton {
    Home,
    Lock,
    Siri,
}

#[vmux_api::payload(Eq)]
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

#[vmux_api::payload(Eq)]
pub struct FileSearchMatch {
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub preview: String,
}

#[vmux_api::payload(Eq)]
pub enum AgentSpaceCommand {
    Create { name: Option<String> },
    Rename { space_id: String, name: String },
    Delete { space_id: String },
}

#[vmux_api::payload(Eq)]
pub struct AgentBookmarkPage {
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
}

#[vmux_api::payload(Eq)]
pub enum AgentBookmarkCommand {
    Add {
        page: AgentBookmarkPage,
        folder: Option<String>,
    },
    Remove {
        uuid: String,
    },
    Pin {
        uuid: String,
    },
    PinUrl {
        page: AgentBookmarkPage,
    },
    Unpin {
        uuid: String,
    },
    CreateFolder {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentCommand {
    InvokeCommand {
        id: String,
        #[rkyv(attr(allow(dead_code)))]
        args: JsonValue,
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
        value: JsonValue,
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
    SpaceCommand(AgentSpaceCommand),
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
    BookmarkCommand(AgentBookmarkCommand),
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

#[vmux_api::payload]
pub enum AgentCommandResult {
    Ok,
    Text(String),
    Layout(crate::protocol::layout::LayoutSnapshot),
    Error(String),
}

#[vmux_api::payload(Copy, Eq, Default)]
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

#[vmux_api::payload]
pub enum AgentRunStatus {
    Streaming,
    Idle,
    Interrupted,
    Errored(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentCommandValidationError {
    EmptyCommandId,
    EmptyShellCommand,
    EmptyBrowserUrl,
    EmptyExtensionSource,
    EmptyTerminalText,
    EmptyPaneId,
    EmptyProfileName,
    EmptySettingsPath,
    EmptyHistoryQuery,
    EmptyStackUrl,
    EmptySpaceName,
    InvalidSpaceRename,
    EmptySpaceId,
    EmptyBookmarkUrl,
    EmptyBookmarkId,
    EmptyBookmarkFolderName,
    EmptyBesideUrl,
    EmptyRunCommand,
    EmptyFilePath,
    EmptyBranch,
    InvalidUserChoice,
    EmptyWorkspacePath,
    InvalidKnowledgeWrite,
    InvalidKnowledgeSearch,
    InvalidKnowledgeRead,
    EmptyAgentPrompt,
}

impl std::fmt::Display for AgentCommandValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::EmptyCommandId => "invoke_command.id is empty",
            Self::EmptyShellCommand => "run_shell.command is empty",
            Self::EmptyBrowserUrl => "browser_navigate.url is empty",
            Self::EmptyExtensionSource => "browser_install_extension.source is empty",
            Self::EmptyTerminalText => "terminal_send.text is empty",
            Self::EmptyPaneId => "focus_pane.pane is empty",
            Self::EmptyProfileName => "rename_profile.name is empty",
            Self::EmptySettingsPath => "update_settings.path is empty",
            Self::EmptyHistoryQuery => "browser_history_search.query is empty",
            Self::EmptyStackUrl => "open_in_new_stack.url is empty",
            Self::EmptySpaceName => "space_command.name is empty",
            Self::InvalidSpaceRename => "space_command rename fields are empty",
            Self::EmptySpaceId => "space_command.space_id is empty",
            Self::EmptyBookmarkUrl => "bookmark_command.url is empty",
            Self::EmptyBookmarkId => "bookmark_command.uuid is empty",
            Self::EmptyBookmarkFolderName => "bookmark_command.name is empty",
            Self::EmptyBesideUrl => "open_beside_me.url is empty",
            Self::EmptyRunCommand => "run.command is empty",
            Self::EmptyFilePath => "file_touched.path is empty",
            Self::EmptyBranch => "create_worktree.branch is empty",
            Self::InvalidUserChoice => {
                "request_user_choice requires a question and 2 to 9 non-empty options"
            }
            Self::EmptyWorkspacePath => "select_project.path is empty",
            Self::InvalidKnowledgeWrite => "write_knowledge requires a non-empty title and content",
            Self::InvalidKnowledgeSearch => {
                "search_knowledge requires a query and limit between 1 and 100"
            }
            Self::InvalidKnowledgeRead => {
                "read_knowledge requires a path and limit between 1 and 2000"
            }
            Self::EmptyAgentPrompt => "new_agent_chat.prompt is empty",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for AgentCommandValidationError {}

pub fn validate_agent_command(command: &AgentCommand) -> Result<(), AgentCommandValidationError> {
    match command {
        AgentCommand::InvokeCommand { id, .. } if id.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyCommandId)
        }
        AgentCommand::RunShell { command, .. } if command.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyShellCommand)
        }
        AgentCommand::BrowserNavigate { url, .. } if url.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyBrowserUrl)
        }
        AgentCommand::BrowserInstallExtension { source } if source.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyExtensionSource)
        }
        AgentCommand::TerminalSend { text, .. } if text.is_empty() => {
            Err(AgentCommandValidationError::EmptyTerminalText)
        }
        AgentCommand::FocusPane { pane } if pane.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyPaneId)
        }
        AgentCommand::RenameProfile { name } if name.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyProfileName)
        }
        AgentCommand::UpdateSettings { path, .. } if path.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptySettingsPath)
        }
        AgentCommand::BrowserHistorySearch { query, .. } if query.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyHistoryQuery)
        }
        AgentCommand::OpenInNewStack { url, .. } if url.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyStackUrl)
        }
        AgentCommand::SpaceCommand(AgentSpaceCommand::Create { name: Some(name) })
            if name.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptySpaceName)
        }
        AgentCommand::SpaceCommand(AgentSpaceCommand::Rename { space_id, name })
            if space_id.trim().is_empty() || name.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::InvalidSpaceRename)
        }
        AgentCommand::SpaceCommand(AgentSpaceCommand::Delete { space_id })
            if space_id.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptySpaceId)
        }
        AgentCommand::BookmarkCommand(AgentBookmarkCommand::Add { page, .. })
        | AgentCommand::BookmarkCommand(AgentBookmarkCommand::PinUrl { page })
            if page.url.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptyBookmarkUrl)
        }
        AgentCommand::BookmarkCommand(AgentBookmarkCommand::Remove { uuid })
        | AgentCommand::BookmarkCommand(AgentBookmarkCommand::Pin { uuid })
        | AgentCommand::BookmarkCommand(AgentBookmarkCommand::Unpin { uuid })
            if uuid.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptyBookmarkId)
        }
        AgentCommand::BookmarkCommand(AgentBookmarkCommand::CreateFolder { name })
            if name.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptyBookmarkFolderName)
        }
        AgentCommand::OpenBeside { url, .. } if url.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyBesideUrl)
        }
        AgentCommand::Run { command, .. }
        | AgentCommand::RunWithPlacementOverride { command, .. }
            if command.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptyRunCommand)
        }
        AgentCommand::FileTouched { path, .. } if path.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyFilePath)
        }
        AgentCommand::CreateWorktreeOnBranch { branch, .. } if branch.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyBranch)
        }
        AgentCommand::RequestUserChoice {
            question, options, ..
        } if question.trim().is_empty()
            || options.len() < 2
            || options.len() > 9
            || options.iter().any(|option| option.trim().is_empty()) =>
        {
            Err(AgentCommandValidationError::InvalidUserChoice)
        }
        AgentCommand::ChooseWorkspaceAtPath { path, .. } if path.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyWorkspacePath)
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
            Err(AgentCommandValidationError::InvalidKnowledgeWrite)
        }
        AgentCommand::SearchKnowledge { query, limit, .. }
            if query.trim().is_empty() || *limit == 0 || *limit > 100 =>
        {
            Err(AgentCommandValidationError::InvalidKnowledgeSearch)
        }
        AgentCommand::ReadKnowledge { path, limit, .. }
            if path.trim().is_empty() || *limit == 0 || *limit > 2_000 =>
        {
            Err(AgentCommandValidationError::InvalidKnowledgeRead)
        }
        AgentCommand::Shared(SharedAgentCommand::NewAgentChat { prompt, .. })
            if prompt.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptyAgentPrompt)
        }
        _ => Ok(()),
    }
}

#[vmux_api::payload(Eq)]
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
