pub mod layout;
pub use layout::{
    Focus, LayoutNode, LayoutSnapshot, NodeKind, SplitDirection, Stack, Tab, format_id, parse_id,
};

use vmux_core::event::{TermCursor, TermLine, TermSelectionRange};

pub use vmux_core::ProcessId;

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
}

pub const AGENT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Stop-recording round-trip bound. `finishWriting` after live encoding is
/// fast, but a large clip's moov flush can take a few seconds. Comfortably
/// under vibe's 60s MCP tool timeout.
pub const RECORD_STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub const AGENT_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

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
}

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentRunStatus {
    Streaming,
    Idle,
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
        AgentCommand::OpenBeside { url, .. } if url.trim().is_empty() => {
            Err("open_beside_me.url is empty")
        }
        AgentCommand::Run { command, .. } if command.trim().is_empty() => {
            Err("run.command is empty")
        }
        _ => Ok(()),
    }
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
    AttachPageAgent {
        sid: String,
    },
    DetachPageAgent {
        sid: String,
    },
    AgentInput {
        sid: String,
        text: String,
    },
    AgentApprove {
        sid: String,
        call_id: String,
        decision: ApprovalDecision,
    },
    ClosePageAgent {
        sid: String,
    },
    AgentToolResult {
        request_id: AgentRequestId,
        content: String,
        is_error: bool,
    },
    Status,
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
        changed_lines: Vec<(u16, TermLine)>,
        cursor: TermCursor,
        cols: u16,
        rows: u16,
        selection: Option<TermSelectionRange>,
        copy_mode: bool,
        full: bool,
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
    AgentDelta {
        sid: String,
        text: String,
    },
    AgentRunStatusChanged {
        sid: String,
        status: AgentRunStatus,
    },
    AgentAwaitingApproval {
        sid: String,
        call_id: String,
        name: String,
        args_json: String,
    },
    AgentToolCall {
        request_id: AgentRequestId,
        sid: String,
        name: String,
        args_json: String,
    },
    AgentMessagesSnapshot {
        sid: String,
        messages_json: String,
    },
    StatusResponse {
        uptime_secs: u64,
        process_count: u32,
    },
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
mod tests {
    use super::*;

    #[test]
    fn agent_request_id_roundtrips() {
        let request_id = AgentRequestId::new();
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&request_id).unwrap();
        let decoded = rkyv::from_bytes::<AgentRequestId, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded, request_id);
    }

    #[test]
    fn empty_browser_navigate_url_is_invalid() {
        assert_eq!(
            validate_agent_command(&AgentCommand::BrowserNavigate {
                url: String::new(),
                pane: None,
            }),
            Err("browser_navigate.url is empty")
        );
    }

    #[test]
    fn empty_agent_shell_command_is_invalid() {
        assert_eq!(
            validate_agent_command(&AgentCommand::RunShell {
                command: String::new(),
                cwd: String::new(),
                mode: AgentShellMode::NewTab,
            }),
            Err("run_shell.command is empty")
        );
    }

    #[test]
    fn empty_terminal_send_text_is_invalid() {
        assert_eq!(
            validate_agent_command(&AgentCommand::TerminalSend {
                text: String::new(),
                terminal: None,
            }),
            Err("terminal_send.text is empty")
        );
    }

    #[test]
    fn empty_rename_profile_name_is_invalid() {
        assert_eq!(
            validate_agent_command(&AgentCommand::RenameProfile {
                name: "  ".to_string(),
            }),
            Err("rename_profile.name is empty")
        );
    }

    #[test]
    fn agent_query_read_layout_rkyv_round_trip() {
        let q = AgentQuery::ReadLayout { anchor: None };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let recovered: AgentQuery =
            rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, AgentQuery::ReadLayout { anchor: None });
    }

    #[test]
    fn agent_query_screenshot_rkyv_round_trip() {
        let q = AgentQuery::Screenshot {
            pane: Some("pane:42".into()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, q);

        let none = AgentQuery::Screenshot { pane: None };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&none).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, none);
    }

    #[test]
    fn agent_query_result_image_rkyv_round_trip() {
        let r = AgentQueryResult::Image {
            path: "/tmp/x.png".into(),
            png: vec![1, 2, 3, 4],
            width: 320,
            height: 200,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let back: AgentQueryResult =
            rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn agent_query_record_start_rkyv_round_trip() {
        let q = AgentQuery::RecordStart {
            gif: true,
            max_secs: 120,
            pane: Some("pane:7".into()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn agent_query_record_stop_rkyv_round_trip() {
        let q = AgentQuery::RecordStop {
            dir: Some("/tmp/out".into()),
            name: None,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn agent_query_result_recording_rkyv_round_trip() {
        let r = AgentQueryResult::Recording {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: Some("/tmp/x.gif".into()),
            duration_ms: 7400,
            bytes: 1_234_567,
            auto_stopped: false,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let back: AgentQueryResult =
            rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn notify_command_rkyv_roundtrip() {
        let cmd = AgentCommand::Notify {
            title: Some("done".to_string()),
            body: None,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let back: AgentCommand =
            rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(cmd, back);
    }

    #[test]
    fn bell_service_message_rkyv_roundtrip() {
        let pid = ProcessId::new();
        let msg = ServiceMessage::Bell { process_id: pid };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let back: ServiceMessage =
            rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        match back {
            ServiceMessage::Bell { process_id } => assert_eq!(process_id, pid),
            _ => panic!("expected ServiceMessage::Bell"),
        }
    }

    #[test]
    fn open_beside_round_trips_and_validates() {
        let cmd = AgentCommand::OpenBeside {
            anchor: ProcessId::new(),
            direction: Some(AgentPaneDirection::Right),
            url: "vmux://terminal/".into(),
            focus: true,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let back: AgentCommand =
            rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, cmd);
        assert!(validate_agent_command(&cmd).is_ok());

        let empty = AgentCommand::OpenBeside {
            anchor: ProcessId::new(),
            direction: Some(AgentPaneDirection::Right),
            url: "  ".into(),
            focus: true,
        };
        assert!(validate_agent_command(&empty).is_err());
    }

    #[test]
    fn agent_command_update_layout_rkyv_round_trip() {
        use crate::protocol::layout::{Focus, LayoutNode, LayoutSnapshot, Tab};
        let cmd = AgentCommand::UpdateLayout {
            layout: LayoutSnapshot {
                tabs: vec![Tab {
                    id: Some("tab:1".into()),
                    name: "X".into(),
                    is_active: true,
                    root: LayoutNode::Pane {
                        id: Some("pane:2".into()),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                }],
                focused: Focus {
                    tab: Some("tab:1".into()),
                    pane: Some("pane:2".into()),
                    stack: None,
                },
            },
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let recovered: AgentCommand =
            rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, cmd);
    }

    #[test]
    fn agent_command_result_roundtrips() {
        for variant in [
            AgentCommandResult::Ok,
            AgentCommandResult::Error("boom".to_string()),
        ] {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&variant).unwrap();
            let decoded =
                rkyv::from_bytes::<AgentCommandResult, rkyv::rancor::Error>(&bytes).unwrap();
            assert_eq!(decoded, variant);
        }
    }

    #[test]
    fn agent_command_result_layout_rkyv_round_trip() {
        let result = AgentCommandResult::Layout(LayoutSnapshot {
            tabs: vec![Tab {
                id: Some("tab:1".into()),
                name: "X".into(),
                is_active: true,
                root: LayoutNode::Pane {
                    id: Some("pane:2".into()),
                    is_zoomed: false,
                    stacks: vec![Stack {
                        id: Some("stack:3".into()),
                        title: "T".into(),
                        url: "https://x".into(),
                        kind: "browser".into(),
                        is_loading: false,
                        icon: vmux_core::PageIcon::None,
                        is_self: false,
                        process_id: None,
                    }],
                },
            }],
            focused: Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:2".into()),
                stack: Some("stack:3".into()),
            },
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&result).unwrap();
        let recovered: AgentCommandResult =
            rkyv::from_bytes::<AgentCommandResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, result);
    }

    #[test]
    fn agent_command_response_messages_roundtrip() {
        let request_id = AgentRequestId::new();
        let client_msg = ClientMessage::AgentCommandResponse {
            request_id,
            result: AgentCommandResult::Ok,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&client_msg).unwrap();
        let _decoded = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();

        let service_msg = ServiceMessage::AgentCommandResult {
            request_id,
            result: AgentCommandResult::Error("nope".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&service_msg).unwrap();
        let _decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
    }

    #[test]
    fn browser_navigate_with_pane_roundtrips() {
        let cmd = AgentCommand::BrowserNavigate {
            url: "https://example.com".to_string(),
            pane: Some("12345".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn browser_navigate_without_pane_roundtrips() {
        let cmd = AgentCommand::BrowserNavigate {
            url: "https://example.com".to_string(),
            pane: None,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn terminal_send_with_terminal_roundtrips() {
        let cmd = AgentCommand::TerminalSend {
            text: "hi".to_string(),
            terminal: Some("67890".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn status_response_roundtrips() {
        let msg = ServiceMessage::StatusResponse {
            uptime_secs: 42,
            process_count: 3,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(matches!(
            decoded,
            ServiceMessage::StatusResponse {
                uptime_secs: 42,
                process_count: 3
            }
        ));
    }

    #[test]
    fn process_created_round_trips_pid() {
        let id = ProcessId::new();
        let msg = ServiceMessage::ProcessCreated {
            process_id: id,
            pid: 12345,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        match decoded {
            ServiceMessage::ProcessCreated { process_id, pid } => {
                assert_eq!(process_id, id);
                assert_eq!(pid, 12345);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn process_create_failed_round_trips_reason() {
        let id = ProcessId::new();
        let msg = ServiceMessage::ProcessCreateFailed {
            process_id: id,
            reason: "missing PID after spawn".into(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        match decoded {
            ServiceMessage::ProcessCreateFailed { process_id, reason } => {
                assert_eq!(process_id, id);
                assert_eq!(reason, "missing PID after spawn");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn update_settings_command_rkyv_roundtrip() {
        let cmd = AgentCommand::UpdateSettings {
            path: "layout.pane.gap".to_string(),
            value_json: "12.0".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn get_settings_query_rkyv_roundtrip() {
        let q = AgentQuery::GetSettings;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let decoded = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, q);
    }

    #[test]
    fn settings_query_result_rkyv_roundtrip() {
        let r = AgentQueryResult::Settings("{\"auto_update\":true}".to_string());
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let decoded = rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, r);
    }

    #[test]
    fn update_settings_validation_rejects_empty_path() {
        let cmd = AgentCommand::UpdateSettings {
            path: "".to_string(),
            value_json: "1".to_string(),
        };
        assert!(validate_agent_command(&cmd).is_err());
    }

    #[test]
    fn page_agent_client_messages_roundtrip() {
        let messages = [
            ClientMessage::SpawnPageAgent {
                sid: "s".into(),
                provider: "anthropic".into(),
                model: "m".into(),
                cwd: "/tmp".into(),
                auto_tools: vec!["list_spaces".into()],
                tools_json: "[]".into(),
            },
            ClientMessage::AttachPageAgent { sid: "s".into() },
            ClientMessage::DetachPageAgent { sid: "s".into() },
            ClientMessage::AgentInput {
                sid: "s".into(),
                text: "hi".into(),
            },
            ClientMessage::AgentApprove {
                sid: "s".into(),
                call_id: "c".into(),
                decision: ApprovalDecision::Allow,
            },
            ClientMessage::ClosePageAgent { sid: "s".into() },
            ClientMessage::AgentToolResult {
                request_id: AgentRequestId::new(),
                content: "ok".into(),
                is_error: false,
            },
        ];
        for msg in messages {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
            rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();
        }
    }

    #[test]
    fn page_agent_service_messages_roundtrip() {
        let messages = [
            ServiceMessage::AgentDelta {
                sid: "s".into(),
                text: "hello".into(),
            },
            ServiceMessage::AgentRunStatusChanged {
                sid: "s".into(),
                status: AgentRunStatus::Streaming,
            },
            ServiceMessage::AgentRunStatusChanged {
                sid: "s".into(),
                status: AgentRunStatus::Errored("boom".into()),
            },
            ServiceMessage::AgentAwaitingApproval {
                sid: "s".into(),
                call_id: "c".into(),
                name: "n".into(),
                args_json: "{}".into(),
            },
            ServiceMessage::AgentToolCall {
                request_id: AgentRequestId::new(),
                sid: "s".into(),
                name: "n".into(),
                args_json: "{}".into(),
            },
            ServiceMessage::AgentMessagesSnapshot {
                sid: "s".into(),
                messages_json: "[]".into(),
            },
        ];
        for msg in messages {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
            rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        }
    }
}
