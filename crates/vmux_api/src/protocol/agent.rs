use super::SharedAgentCommand;
use crate::{ProcessId, json::JsonValue};

#[vmux_api::contract(Copy, Eq, Hash)]
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

#[vmux_api::contract(Copy, Eq)]
pub enum AgentShellMode {
    NewTab,
    Active,
}

#[vmux_api::contract(Copy, Eq)]
pub enum AgentPaneDirection {
    Top,
    Right,
    Bottom,
    Left,
}

#[vmux_api::contract(Copy, Eq)]
pub enum ManagedMcpTransport {
    Stdio,
    Http,
    Sse,
}

#[vmux_api::contract(Eq)]
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

#[vmux_api::contract(Copy, Eq)]
pub enum PlacementMode {
    Auto,
    Split,
    Stack,
}

#[vmux_api::contract(Copy, Eq)]
pub enum FileTouchKind {
    Read,
    Edit,
}

#[vmux_api::contract(Copy, Eq)]
pub enum SimulatorButton {
    Home,
    Lock,
    Siri,
}

#[vmux_api::contract(Eq)]
pub struct FileSearchMatch {
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub preview: String,
}

#[vmux_api::contract(Eq)]
pub struct AgentBookmarkPage {
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
}

#[vmux_api::contract]
pub struct AgentInvokeCommand {
    pub id: String,
    #[rkyv(attr(allow(dead_code)))]
    pub args: JsonValue,
}

#[vmux_api::contract]
pub struct AgentNewTerminalTab {
    pub cwd: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[vmux_api::contract]
pub struct AgentRunShell {
    pub command: String,
    pub cwd: String,
    pub mode: AgentShellMode,
}

#[vmux_api::contract]
pub struct AgentBrowserNavigate {
    pub url: String,
    pub pane: Option<String>,
}

#[vmux_api::contract]
pub struct AgentBrowserInstallExtension {
    pub source: String,
}

#[vmux_api::contract]
pub struct AgentTerminalSend {
    pub text: String,
    pub terminal: Option<String>,
}

#[vmux_api::contract]
pub struct AgentFocusPane {
    pub pane: String,
}

#[vmux_api::contract]
pub struct AgentRenameProfile {
    pub name: String,
}

#[vmux_api::contract]
pub struct AgentUpdateSettings {
    pub path: String,
    pub value: JsonValue,
}

#[vmux_api::contract]
pub struct AgentUpdateLayout {
    pub layout: crate::protocol::layout::LayoutSnapshot,
}

#[vmux_api::contract]
pub struct AgentBrowserHistoryStep {
    pub pane: Option<String>,
}

#[vmux_api::contract]
pub struct AgentBrowserHistorySearch {
    pub query: String,
    pub limit: u32,
}

#[vmux_api::contract]
pub struct AgentOpenInNewStack {
    pub url: String,
}

#[vmux_api::contract]
pub struct AgentSpaceCreate {
    pub name: Option<String>,
}

#[vmux_api::contract]
pub struct AgentSpaceRename {
    pub space_id: String,
    pub name: String,
}

#[vmux_api::contract]
pub struct AgentSpaceDelete {
    pub space_id: String,
}

#[vmux_api::contract]
pub struct AgentOpenBeside {
    pub anchor: ProcessId,
    pub direction: Option<AgentPaneDirection>,
    pub url: String,
    pub focus: bool,
}

#[vmux_api::contract]
pub struct AgentRun {
    pub anchor: ProcessId,
    pub command: String,
    pub direction: AgentPaneDirection,
    pub focus: bool,
    pub beside: Option<ProcessId>,
    pub mode: PlacementMode,
    pub terminal: Option<ProcessId>,
    pub done_marker: Option<String>,
}

#[vmux_api::contract]
pub struct AgentNotify {
    pub title: Option<String>,
    pub body: Option<String>,
}

#[vmux_api::contract]
pub struct AgentFileTouched {
    pub anchor: ProcessId,
    pub path: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub end_col: Option<u32>,
    pub kind: FileTouchKind,
}

#[vmux_api::contract(Copy, Eq)]
pub struct AgentCreateWorktree {
    pub anchor: ProcessId,
}

#[vmux_api::contract(Copy, Eq)]
pub struct AgentTurnEnded {
    pub anchor: ProcessId,
}

#[vmux_api::contract(Copy, Eq)]
pub struct AgentResumeInAcp {
    pub anchor: ProcessId,
}

#[vmux_api::contract(Copy, Eq)]
pub struct AgentChooseWorkspace {
    pub anchor: ProcessId,
}

#[vmux_api::contract]
pub struct AgentCreateWorktreeOnBranch {
    pub anchor: ProcessId,
    pub branch: String,
    pub project: Option<String>,
}

#[vmux_api::contract]
pub struct AgentBookmarkAdd {
    pub page: AgentBookmarkPage,
    pub folder: Option<String>,
}

#[vmux_api::contract]
pub struct AgentBookmarkId {
    pub uuid: String,
}

#[vmux_api::contract]
pub struct AgentBookmarkPinUrl {
    pub page: AgentBookmarkPage,
}

#[vmux_api::contract]
pub struct AgentBookmarkFolderCreate {
    pub name: String,
}

#[vmux_api::contract]
pub struct AgentRequestUserChoice {
    pub anchor: ProcessId,
    pub question: String,
    pub options: Vec<String>,
}

#[vmux_api::contract]
pub struct AgentChooseWorkspaceAtPath {
    pub anchor: ProcessId,
    pub path: String,
}

#[vmux_api::contract]
pub struct AgentPrepareWorktree {
    pub anchor: ProcessId,
    pub path: Option<String>,
    pub task: Option<String>,
    pub create: bool,
}

#[vmux_api::contract]
pub struct AgentFileSearch {
    pub anchor: ProcessId,
    pub root: String,
    pub query: String,
    pub matches: Vec<FileSearchMatch>,
}

#[vmux_api::contract]
pub struct AgentSetConversationTitle {
    pub anchor: ProcessId,
    pub title: String,
}

#[vmux_api::contract]
pub struct AgentWriteKnowledge {
    pub anchor: ProcessId,
    pub path: Option<String>,
    pub title: String,
    pub content: String,
}

#[vmux_api::contract]
pub struct AgentSearchKnowledge {
    pub anchor: ProcessId,
    pub query: String,
    pub limit: u16,
}

#[vmux_api::contract]
pub struct AgentReadKnowledge {
    pub anchor: ProcessId,
    pub path: String,
    pub line: u32,
    pub limit: u32,
}

#[vmux_api::contract]
pub enum AgentCommand {
    InvokeCommand(AgentInvokeCommand),
    NewTerminalTab(AgentNewTerminalTab),
    RunShell(AgentRunShell),
    BrowserNavigate(AgentBrowserNavigate),
    BrowserInstallExtension(AgentBrowserInstallExtension),
    TerminalSend(AgentTerminalSend),
    FocusPane(AgentFocusPane),
    RenameProfile(AgentRenameProfile),
    UpdateSettings(AgentUpdateSettings),
    UpdateLayout(AgentUpdateLayout),
    BrowserGoBack(AgentBrowserHistoryStep),
    BrowserGoForward(AgentBrowserHistoryStep),
    BrowserHistorySearch(AgentBrowserHistorySearch),
    OpenInNewStack(AgentOpenInNewStack),
    SpaceCreate(AgentSpaceCreate),
    SpaceRename(AgentSpaceRename),
    SpaceDelete(AgentSpaceDelete),
    OpenBeside(AgentOpenBeside),
    Run(AgentRun),
    Notify(AgentNotify),
    FileTouched(AgentFileTouched),
    CreateWorktree(AgentCreateWorktree),
    TurnEnded(AgentTurnEnded),
    RunWithPlacementOverride(AgentRun),
    ResumeInAcp(AgentResumeInAcp),
    ChooseWorkspace(AgentChooseWorkspace),
    CreateWorktreeOnBranch(AgentCreateWorktreeOnBranch),
    BookmarkAdd(AgentBookmarkAdd),
    BookmarkRemove(AgentBookmarkId),
    BookmarkPin(AgentBookmarkId),
    BookmarkPinUrl(AgentBookmarkPinUrl),
    BookmarkUnpin(AgentBookmarkId),
    BookmarkFolderCreate(AgentBookmarkFolderCreate),
    RequestUserChoice(AgentRequestUserChoice),
    ChooseWorkspaceAtPath(AgentChooseWorkspaceAtPath),
    PrepareWorktree(AgentPrepareWorktree),
    FileSearch(AgentFileSearch),
    SetConversationTitle(AgentSetConversationTitle),
    WriteKnowledge(AgentWriteKnowledge),
    SearchKnowledge(AgentSearchKnowledge),
    ReadKnowledge(AgentReadKnowledge),
    Shared(SharedAgentCommand),
}

pub const AGENT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub const RECORD_STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub const AGENT_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub const BROWSER_NAVIGATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub const AGENT_TOOL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[vmux_api::contract]
pub enum AgentCommandResult {
    Ok,
    Text(String),
    Layout(crate::protocol::layout::LayoutSnapshot),
    Error(String),
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum ApprovalDecision {
    Allow,
    #[default]
    Deny,
    AllowAlways,
}

#[vmux_api::contract]
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
        AgentCommand::InvokeCommand(request) if request.id.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyCommandId)
        }
        AgentCommand::RunShell(request) if request.command.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyShellCommand)
        }
        AgentCommand::BrowserNavigate(request) if request.url.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyBrowserUrl)
        }
        AgentCommand::BrowserInstallExtension(request) if request.source.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyExtensionSource)
        }
        AgentCommand::TerminalSend(request) if request.text.is_empty() => {
            Err(AgentCommandValidationError::EmptyTerminalText)
        }
        AgentCommand::FocusPane(request) if request.pane.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyPaneId)
        }
        AgentCommand::RenameProfile(request) if request.name.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyProfileName)
        }
        AgentCommand::UpdateSettings(request) if request.path.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptySettingsPath)
        }
        AgentCommand::BrowserHistorySearch(request) if request.query.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyHistoryQuery)
        }
        AgentCommand::OpenInNewStack(request) if request.url.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyStackUrl)
        }
        AgentCommand::SpaceCreate(AgentSpaceCreate { name: Some(name) })
            if name.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptySpaceName)
        }
        AgentCommand::SpaceRename(AgentSpaceRename { space_id, name })
            if space_id.trim().is_empty() || name.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::InvalidSpaceRename)
        }
        AgentCommand::SpaceDelete(AgentSpaceDelete { space_id }) if space_id.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptySpaceId)
        }
        AgentCommand::BookmarkAdd(AgentBookmarkAdd { page, .. })
        | AgentCommand::BookmarkPinUrl(AgentBookmarkPinUrl { page })
            if page.url.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptyBookmarkUrl)
        }
        AgentCommand::BookmarkRemove(AgentBookmarkId { uuid })
        | AgentCommand::BookmarkPin(AgentBookmarkId { uuid })
        | AgentCommand::BookmarkUnpin(AgentBookmarkId { uuid })
            if uuid.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptyBookmarkId)
        }
        AgentCommand::BookmarkFolderCreate(AgentBookmarkFolderCreate { name })
            if name.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptyBookmarkFolderName)
        }
        AgentCommand::OpenBeside(request) if request.url.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyBesideUrl)
        }
        AgentCommand::Run(request) | AgentCommand::RunWithPlacementOverride(request)
            if request.command.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::EmptyRunCommand)
        }
        AgentCommand::FileTouched(request) if request.path.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyFilePath)
        }
        AgentCommand::CreateWorktreeOnBranch(request) if request.branch.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyBranch)
        }
        AgentCommand::RequestUserChoice(request)
            if request.question.trim().is_empty()
                || request.options.len() < 2
                || request.options.len() > 9
                || request
                    .options
                    .iter()
                    .any(|option| option.trim().is_empty()) =>
        {
            Err(AgentCommandValidationError::InvalidUserChoice)
        }
        AgentCommand::ChooseWorkspaceAtPath(request) if request.path.trim().is_empty() => {
            Err(AgentCommandValidationError::EmptyWorkspacePath)
        }
        AgentCommand::WriteKnowledge(request)
            if request
                .path
                .as_ref()
                .is_some_and(|path| path.trim().is_empty())
                || request.title.trim().is_empty()
                || request.content.trim().is_empty() =>
        {
            Err(AgentCommandValidationError::InvalidKnowledgeWrite)
        }
        AgentCommand::SearchKnowledge(request)
            if request.query.trim().is_empty() || request.limit == 0 || request.limit > 100 =>
        {
            Err(AgentCommandValidationError::InvalidKnowledgeSearch)
        }
        AgentCommand::ReadKnowledge(request)
            if request.path.trim().is_empty() || request.limit == 0 || request.limit > 2_000 =>
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

#[vmux_api::contract(Eq)]
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
