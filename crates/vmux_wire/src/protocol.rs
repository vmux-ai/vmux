pub mod layout;
pub mod shared;
pub use layout::{
    Focus, LayoutNode, LayoutSnapshot, NodeKind, SplitDirection, Stack, Tab, format_id, parse_id,
};
pub use shared::{
    AgentAction, SharedAgentCommand, SharedEvent, SharedFailure, SharedMessage, SharedResponse,
};

use crate::{TermCursor, TermLine, TermSelectionRange};

pub use crate::ProcessId;

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

/// How a spawned page is placed relative to its anchor pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum PlacementMode {
    /// Default: don't spawn a new pane unless necessary. Reuse the agent's
    /// existing terminal region (stack the new terminal into it); split one pane
    /// off the agent only when no region exists yet.
    Auto,
    /// New pane split off the anchor pane (X/Y), in the given direction.
    Split,
    /// New stack added to the anchor pane itself (Z); the pane keeps its size.
    Stack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum FileTouchKind {
    Read,
    Edit,
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
        /// Anchor a newly opened terminal next to this page (a terminal's
        /// `ProcessId`); `None` anchors to the agent's own page. Ignored when
        /// `terminal` is set (reuse).
        beside: Option<ProcessId>,
        /// How a newly opened terminal is placed relative to its anchor pane
        /// (split into a new pane, or stacked into the anchor pane). Ignored when
        /// `terminal` is set.
        mode: PlacementMode,
        /// Run in this existing terminal (its `ProcessId`); `None` opens a new
        /// terminal beside the agent.
        terminal: Option<ProcessId>,
        /// When set, the GUI appends a shell-aware completion print using this
        /// token so the caller can detect command completion + exit code in the
        /// terminal output. `None` keeps the legacy fire-and-forget behavior.
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
        /// 0-based start/end columns of the match on `line`, for highlighting
        /// (e.g. a grep hit). `None` = no column highlight (plain open/scroll).
        col: Option<u32>,
        end_col: Option<u32>,
        kind: FileTouchKind,
    },
    /// Create (or reuse) an isolated git worktree for the calling agent's tab and return its
    /// path. Resolved to the tab via `anchor`. Appended at the end so rkyv's positional enum
    /// discriminants stay stable for existing variants (the daemon is long-lived across GUI
    /// updates, so shifting a discriminant would break wire compat mid-upgrade).
    CreateWorktree {
        anchor: ProcessId,
    },
    /// The calling CLI agent finished a turn (fired from its `Stop` hook). Resolved to the agent
    /// via `anchor`; the GUI raises `AgentAttention` so the follow-pane auto-tidy and the
    /// done-dot fire at turn-end (the terminal bell only fires on idle/permission, not turn-end).
    /// Appended at the end to keep rkyv's positional enum discriminants stable.
    TurnEnded {
        anchor: ProcessId,
    },
    /// Run with caller-requested pane placement. Appended to keep existing rkyv variant layouts
    /// and positional discriminants stable across daemon upgrades.
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
    /// Replace the calling CLI session with its ACP runtime, preserving session id and cwd.
    /// Appended to keep existing rkyv variant layouts and positional discriminants stable.
    ResumeInAcp {
        anchor: ProcessId,
    },
    /// Ask the user to select a project directory for the calling agent's tab.
    /// Appended to preserve existing positional enum discriminants.
    ChooseWorkspace {
        anchor: ProcessId,
    },
    /// Create an isolated worktree on an exact user-selected branch.
    /// Appended to preserve existing positional enum discriminants.
    CreateWorktreeOnBranch {
        anchor: ProcessId,
        branch: String,
    },
    BookmarkCommand {
        command: String,
        uuid: Option<String>,
        name: Option<String>,
        url: Option<String>,
        title: Option<String>,
        favicon_url: Option<String>,
    },
    /// Show a native multiple-choice prompt and resume the same agent session with the answer.
    /// Appended to preserve existing positional enum discriminants.
    RequestUserChoice {
        anchor: ProcessId,
        question: String,
        options: Vec<String>,
    },
    /// Select a known project path, falling back to the native folder picker when it is invalid.
    /// Appended to preserve existing positional enum discriminants.
    ChooseWorkspaceAtPath {
        anchor: ProcessId,
        path: String,
    },
    /// Prepare a worktree immediately before mutation, reusing a known checkout when possible.
    /// Appended to preserve existing positional enum discriminants.
    PrepareWorktree {
        anchor: ProcessId,
        path: Option<String>,
        task: Option<String>,
        create: bool,
    },
    /// Forward global-search matches to the editor. Appended to preserve existing positional enum
    /// discriminants.
    FileSearch {
        anchor: ProcessId,
        root: String,
        query: String,
        matches: Vec<FileSearchMatch>,
    },
    /// Replace the generated conversation title. Appended to preserve existing positional enum
    /// discriminants.
    SetConversationTitle {
        anchor: ProcessId,
        title: String,
    },
    /// Write a user-approved Markdown note into the vmux Knowledge base.
    /// Appended to preserve existing positional enum discriminants.
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
    /// The commands a remote peer may also issue. Appended last so the preceding positional
    /// rkyv discriminants keep their existing values.
    Shared(SharedAgentCommand),
}

pub const AGENT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Stop-recording round-trip bound. `finishWriting` after live encoding is
/// fast, but a large clip's moov flush can take a few seconds. Comfortably
/// under vibe's 60s MCP tool timeout.
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

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQuery {
    ReadLayout {
        anchor: Option<ProcessId>,
    },
    ReadTerminal {
        process_id: ProcessId,
    },
    /// Like `ReadTerminal` but returns the full scrollback history plus the
    /// visible screen as plain text (used to capture a command's complete
    /// output, not just the current viewport).
    ReadTerminalFull {
        process_id: ProcessId,
    },
    CommandExit {
        process_id: ProcessId,
    },
    /// Last agent `run` completion for this process, correlated by the per-run
    /// token carried in the service run-marker escape.
    RunCompletion {
        process_id: ProcessId,
    },
    GetSettings,
    ListSpaces,
    Screenshot {
        pane: Option<String>,
    },
    BrowserSnapshot {
        pane: Option<String>,
        anchor: Option<ProcessId>,
    },
    BrowserScroll {
        pane: Option<String>,
        to: Option<String>,
        delta: Option<i32>,
        anchor: Option<ProcessId>,
    },
    RecordStart {
        gif: bool,
        max_secs: u32,
        pane: Option<String>,
    },
    RecordStop {
        dir: Option<String>,
        name: Option<String>,
    },
    BookmarkList,
}

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQueryResult {
    Layout(crate::protocol::layout::LayoutSnapshot),
    Text(String),
    Settings(String),
    Spaces(String),
    CommandExit {
        seq: u64,
        exit: Option<i32>,
    },
    RunCompletion {
        token: Option<String>,
        exit: Option<i32>,
    },
    Image {
        path: String,
        png: Vec<u8>,
        width: u32,
        height: u32,
    },
    Recording {
        mp4_path: String,
        gif_path: Option<String>,
        duration_ms: u64,
        bytes: u64,
        auto_stopped: bool,
    },
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ApprovalDecision {
    Allow,
    Deny,
    AllowAlways,
}

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentRunStatus {
    Streaming,
    Idle,
    /// The user interrupted the in-flight turn (Esc / Ctrl+C / Stop). Distinct from `Idle`
    /// so the UI can mark the stopped turn and pause the queue instead of auto-advancing.
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

/// A local file attached to an agent prompt.
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

/// Messages sent from the GUI client to the service.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ClientMessage {
    CreateProcess {
        process_id: ProcessId,
        command: String,
        args: Vec<String>,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    },
    AttachProcess {
        process_id: ProcessId,
    },
    DetachProcess {
        process_id: ProcessId,
    },
    ProcessInput {
        process_id: ProcessId,
        data: Vec<u8>,
    },
    MouseWheel {
        process_id: ProcessId,
        up: bool,
        col: u16,
        row: u16,
        modifiers: u8,
    },
    ScrollWindow {
        process_id: ProcessId,
        top_row: u32,
        follow: bool,
    },
    ResizeProcess {
        process_id: ProcessId,
        cols: u16,
        rows: u16,
    },
    ListProcesses,
    KillProcess {
        process_id: ProcessId,
    },
    RequestSnapshot {
        process_id: ProcessId,
    },
    SetSelection {
        process_id: ProcessId,
        range: Option<TermSelectionRange>,
    },
    ExtendSelectionTo {
        process_id: ProcessId,
        col: u16,
        row: u16,
    },
    SelectWordAt {
        process_id: ProcessId,
        col: u16,
        row: u16,
    },
    SelectLineAt {
        process_id: ProcessId,
        row: u16,
    },
    GetSelectionText {
        process_id: ProcessId,
    },
    EnterCopyMode {
        process_id: ProcessId,
    },
    ExitCopyMode {
        process_id: ProcessId,
    },
    CopyModeKey {
        process_id: ProcessId,
        key: CopyModeKey,
    },
    SubscribeAgentCommands,
    AgentCommand {
        request_id: AgentRequestId,
        anchor: Option<ProcessId>,
        command: AgentCommand,
    },
    Shutdown,
    AgentQuery {
        request_id: AgentRequestId,
        query: AgentQuery,
    },
    AgentQueryResponse {
        request_id: AgentRequestId,
        result: AgentQueryResult,
    },
    AgentCommandResponse {
        request_id: AgentRequestId,
        result: AgentCommandResult,
    },
    SpawnPageAgent {
        sid: String,
        provider: String,
        model: String,
        cwd: String,
        auto_tools: Vec<String>,
        tools_json: String,
    },
    DetachPageAgent {
        sid: String,
    },
    /// Select a model exposed by an ACP session's model configuration option.
    AcpSetModel {
        sid: String,
        request_id: u64,
        config_id: String,
        model_id: String,
    },
    ClosePageAgent {
        sid: String,
    },
    AgentToolResult {
        request_id: AgentRequestId,
        content: String,
        is_error: bool,
    },
    /// Spawn an ACP (Agent Client Protocol) agent subprocess and start a session.
    SpawnAcpAgent {
        sid: String,
        agent_id: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: String,
        /// Anchor that ties this agent's vmux_mcp tool calls back to its pane.
        anchor: ProcessId,
        /// The `vmux mcp --anchor …` sidecar to hand the agent as an MCP server (scope C).
        mcp_command: Option<String>,
        mcp_args: Vec<String>,
        /// When set, resume this agent-assigned ACP session id via `session/load` instead of
        /// starting a fresh session (gated on the agent's `loadSession` capability).
        resume_acp_session_id: Option<String>,
        /// MCP servers imported into vmux Registry and managed for every launched agent.
        managed_mcp_servers: Vec<ManagedMcpServer>,
        /// Launch-time reasoning-effort level for this session (agent-specific; e.g. Claude
        /// forwards it through `claudeCode.options.effort`). `None` = the agent's own default.
        effort: Option<String>,
    },
    Status,
    /// Update the host-side working directory used by an existing ACP session.
    RebindAcpWorkspace {
        sid: String,
        cwd: String,
    },
    /// The operations a remote peer may also perform. Appended last so the preceding positional
    /// rkyv discriminants keep their existing values.
    Shared(SharedMessage),
}

impl ClientMessage {
    /// Address a prompt to a session.
    pub fn agent_input(
        sid: String,
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
    ) -> Self {
        SharedMessage::agent(
            sid,
            AgentAction::Input {
                text,
                context,
                attachments,
            },
        )
        .into()
    }
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

/// Returns the visible user prompt from a vmux private-context envelope.
pub fn extract_display_prompt(prompt: &str) -> Option<&str> {
    split_private_context_prompt(prompt).map(|(_, display)| display)
}

/// Returns the private context and visible prompt from a vmux context envelope.
pub fn split_private_context_prompt(prompt: &str) -> Option<(&str, &str)> {
    split_length_delimited_private_context(prompt).or_else(|| {
        let body = private_context_body(prompt)?;
        let separator = format!("{PRIVATE_CONTEXT_CLOSING_TAG}{PRIVATE_CONTEXT_PROMPT_MARKER}");
        body.rsplit_once(&separator)
    })
}

/// Returns whether text contains a complete vmux private-context envelope.
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

/// Vim-style visual/copy-mode action sent by the GUI to the service.
///
/// All movement keys (Left/Right/Up/Down/LineStart/LineEnd/PageUp/PageDown)
/// reposition the copy-mode cursor. If visual selection is active, movement
/// also extends the selection to the new cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum CopyModeKey {
    /// Move cursor one cell left (clamped to col 0).
    Left,
    /// Move cursor one cell right (clamped to last column).
    Right,
    /// Move cursor one row up (clamped to row 0).
    Up,
    /// Move cursor one row down (clamped to last row).
    Down,
    /// Jump cursor to column 0 of the current row.
    LineStart,
    /// Jump cursor to the last column of the current row.
    LineEnd,
    /// Jump cursor to the last non-blank cell of the current row (`g_`).
    LastNonBlank,
    /// Jump cursor to the first non-blank cell of the current row (`^`).
    FirstNonBlank,
    /// Move to the next vi word start (`w`).
    WordForward,
    /// Move to the next whitespace-delimited WORD start (`W`).
    BigWordForward,
    /// Move to the previous vi word start (`b`).
    WordBackward,
    /// Move to the previous whitespace-delimited WORD start (`B`).
    BigWordBackward,
    /// Move to the next vi word end (`e`).
    WordEndForward,
    /// Move to the next whitespace-delimited WORD end (`E`).
    BigWordEndForward,
    /// Move to the previous vi word end (`ge`).
    WordEndBackward,
    /// Move to the previous whitespace-delimited WORD end (`gE`).
    BigWordEndBackward,
    /// Move to the first visible row (`gg`).
    Top,
    /// Move to the last visible row (`G`).
    Bottom,
    /// Move to the top visible row (`H`).
    ScreenTop,
    /// Move to the middle visible row (`M`).
    ScreenMiddle,
    /// Move to the bottom visible row (`L`).
    ScreenBottom,
    /// Move to the previous paragraph/blank-line boundary (`{`).
    PrevParagraph,
    /// Move to the next paragraph/blank-line boundary (`}`).
    NextParagraph,
    /// Find a character forward on the current line (`f{char}`).
    FindForward(char),
    /// Find a character backward on the current line (`F{char}`).
    FindBackward(char),
    /// Move until before a character forward on the current line (`t{char}`).
    TillForward(char),
    /// Move until after a character backward on the current line (`T{char}`).
    TillBackward(char),
    /// Repeat the last find/till motion (`;`).
    RepeatFind,
    /// Repeat the last find/till motion in reverse (`,`).
    RepeatFindReverse,
    /// Swap visual anchor and cursor (`o`).
    SwapSelectionEnds,
    /// Move cursor up by half a screen.
    PageUp,
    /// Move cursor down by half a screen.
    PageDown,
    /// Re-anchor the selection at the current cursor position. Subsequent
    /// movement keys extend the selection from this anchor.
    StartSelection,
    /// Select full lines from the current cursor row. Subsequent movement
    /// extends the linewise selection by row.
    StartLineSelection,
    /// Return the current selection text and exit copy mode.
    Copy,
    /// Discard any selection and exit copy mode.
    Exit,
}

/// Messages sent from the service to the GUI client.
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum CommandLifecycleKind {
    Started,
    Ended { exit_code: Option<i32> },
}

#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ServiceMessage {
    ProcessCreated {
        process_id: ProcessId,
        pid: u32,
    },
    ProcessCreateFailed {
        process_id: ProcessId,
        reason: String,
    },
    ProcessOutput {
        process_id: ProcessId,
        data: Vec<u8>,
    },
    ViewportPatch {
        process_id: ProcessId,
        changed_lines: Vec<(u32, TermLine)>,
        cursor: TermCursor,
        cols: u16,
        rows: u16,
        selection: Option<TermSelectionRange>,
        copy_mode: bool,
        full: bool,
        first_row: u32,
        total_rows: u32,
        alt: bool,
        mouse: bool,
        evicted_total: u64,
    },
    ProcessExited {
        process_id: ProcessId,
        exit_code: Option<i32>,
    },
    ProcessTitle {
        process_id: ProcessId,
        title: String,
    },
    CommandLifecycle {
        process_id: ProcessId,
        kind: CommandLifecycleKind,
    },
    ProcessList {
        processes: Vec<ProcessInfo>,
    },
    Snapshot {
        process_id: ProcessId,
        lines: Vec<TermLine>,
        cursor: TermCursor,
        cols: u16,
        rows: u16,
    },
    Error {
        message: String,
    },
    SelectionText {
        process_id: ProcessId,
        text: String,
    },
    TerminalMode {
        process_id: ProcessId,
        mouse_capture: bool,
        copy_mode: bool,
        alt_screen: bool,
        focus_reporting: bool,
    },
    AgentCommand {
        request_id: AgentRequestId,
        anchor: Option<ProcessId>,
        command: AgentCommand,
    },
    AgentQuery {
        request_id: AgentRequestId,
        query: AgentQuery,
    },
    AgentQueryResult {
        request_id: AgentRequestId,
        result: AgentQueryResult,
    },
    AgentCommandResult {
        request_id: AgentRequestId,
        result: AgentCommandResult,
    },
    Bell {
        process_id: ProcessId,
    },
    AgentToolCall {
        request_id: AgentRequestId,
        sid: String,
        name: String,
        args_json: String,
    },
    /// An ACP agent created a terminal; the GUI spawns a visible pane bound to `process_id`.
    AcpTerminalCreated {
        sid: String,
        terminal_id: String,
        process_id: ProcessId,
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
    },
    /// An ACP tool-call carries a proposed edit; the GUI shows it as a pending diff overlay.
    AcpProposedDiff {
        sid: String,
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    StatusResponse {
        uptime_secs: u64,
        process_count: u32,
    },
    /// The ACP agent's session was created (or loaded); carries the agent-assigned session id so
    /// the GUI can persist it (in the pane url) for a later `session/load` resume.
    AcpSessionCreated {
        sid: String,
        acp_session_id: String,
    },
    /// Completion of a model selection request, correlated by `request_id`.
    AcpModelSelectionResult {
        sid: String,
        request_id: u64,
        model_id: String,
        succeeded: bool,
    },
    /// The events a remote peer may also receive. Appended last so the preceding positional
    /// rkyv discriminants keep their existing values.
    Shared(SharedEvent),
}

/// One model exposed by an ACP session configuration selector.
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct AcpModelOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// Metadata about a process, returned in ProcessList.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ProcessInfo {
    pub id: ProcessId,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub created_at_secs: u64,
}

#[cfg(test)]
#[path = "protocol.test.rs"]
mod tests;
