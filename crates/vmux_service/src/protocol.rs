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
    TerminalSend {
        text: String,
        terminal: Option<String>,
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
}

pub const AGENT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub const AGENT_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentCommandResult {
    Ok,
    Layout(crate::protocol::layout::LayoutSnapshot),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQuery {
    ReadLayout,
    GetSettings,
}

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQueryResult {
    Layout(crate::protocol::layout::LayoutSnapshot),
    Settings(String),
    Error(String),
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
        AgentCommand::TerminalSend { text, .. } if text.is_empty() => {
            Err("terminal_send.text is empty")
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
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ServiceMessage {
    ProcessCreated {
        process_id: ProcessId,
        pid: u32,
    },
    ProcessCreateFailed {
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
    },
    AgentCommand {
        request_id: AgentRequestId,
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
    fn agent_query_read_layout_rkyv_round_trip() {
        let q = AgentQuery::ReadLayout;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let recovered: AgentQuery =
            rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, AgentQuery::ReadLayout);
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
                        favicon_url: String::new(),
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
        let msg = ServiceMessage::ProcessCreateFailed {
            reason: "missing PID after spawn".into(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        match decoded {
            ServiceMessage::ProcessCreateFailed { reason } => {
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
}
