use vmux_terminal::event::{TermCursor, TermLine, TermSelectionRange};

/// Unique identifier for a service-managed terminal process.
/// Stored as raw bytes for rkyv compatibility.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ProcessId(pub [u8; 16]);

impl Default for ProcessId {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessId {
    pub fn new() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    pub fn to_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::from_bytes(self.0)
    }

    pub fn from_uuid(u: uuid::Uuid) -> Self {
        Self(*u.as_bytes())
    }
}

impl std::fmt::Display for ProcessId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_uuid())
    }
}

impl std::str::FromStr for ProcessId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_uuid(s.parse()?))
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentCommand {
    AppCommand {
        id: String,
    },
    NewTerminalTab {
        cwd: String,
    },
    RunShell {
        command: String,
        cwd: String,
        mode: AgentShellMode,
    },
    BrowserNavigate {
        url: String,
    },
    TerminalSend {
        text: String,
    },
}

pub const AGENT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQuery {
    GetState,
    ListTabs,
    ListSpaces,
    ListTerminals,
    GetFocused,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TabInfo {
    pub id: String,
    pub title: String,
    pub url: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TerminalInfo {
    pub id: String,
    pub cwd: String,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct PaneInfo {
    pub id: String,
    pub tabs: Vec<TabInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SpaceInfo {
    pub id: String,
    pub name: String,
    pub panes: Vec<PaneInfo>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FocusedInfo {
    pub space: Option<String>,
    pub pane: Option<String>,
    pub tab: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct StateSnapshot {
    pub spaces: Vec<SpaceInfo>,
    pub focused: FocusedInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQueryResult {
    State(StateSnapshot),
    Tabs(Vec<TabInfo>),
    Spaces(Vec<SpaceInfo>),
    Terminals(Vec<TerminalInfo>),
    Focused(FocusedInfo),
    Error(String),
}

pub fn validate_agent_command(command: &AgentCommand) -> Result<(), &'static str> {
    match command {
        AgentCommand::AppCommand { id } if id.trim().is_empty() => Err("app_command.id is empty"),
        AgentCommand::RunShell { command, .. } if command.trim().is_empty() => {
            Err("run_shell.command is empty")
        }
        AgentCommand::BrowserNavigate { url } if url.trim().is_empty() => {
            Err("browser_navigate.url is empty")
        }
        AgentCommand::TerminalSend { text } if text.is_empty() => {
            Err("terminal_send.text is empty")
        }
        _ => Ok(()),
    }
}

/// Messages sent from the GUI client to the service.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ClientMessage {
    CreateProcess {
        shell: String,
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
    AgentCommandAccepted {
        request_id: AgentRequestId,
    },
    AgentQuery {
        request_id: AgentRequestId,
        query: AgentQuery,
    },
    AgentQueryResult {
        request_id: AgentRequestId,
        result: AgentQueryResult,
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
            validate_agent_command(&AgentCommand::BrowserNavigate { url: String::new() }),
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
            }),
            Err("terminal_send.text is empty")
        );
    }

    #[test]
    fn agent_query_roundtrips() {
        let q = AgentQuery::ListTabs;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let decoded = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, q);
    }

    #[test]
    fn agent_query_result_state_roundtrips() {
        let snapshot = StateSnapshot {
            spaces: vec![SpaceInfo {
                id: "1v0".to_string(),
                name: "Main".to_string(),
                panes: vec![PaneInfo {
                    id: "2v0".to_string(),
                    tabs: vec![TabInfo {
                        id: "3v0".to_string(),
                        title: "Hello".to_string(),
                        url: "https://example.com".to_string(),
                        kind: "browser".to_string(),
                    }],
                }],
                active: true,
            }],
            focused: FocusedInfo {
                space: Some("1v0".to_string()),
                pane: Some("2v0".to_string()),
                tab: Some("3v0".to_string()),
            },
        };
        let result = AgentQueryResult::State(snapshot.clone());
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&result).unwrap();
        let decoded = rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, result);
    }

    #[test]
    fn agent_query_message_variants_roundtrip() {
        let request_id = AgentRequestId::new();
        let client_msg = ClientMessage::AgentQuery {
            request_id,
            query: AgentQuery::GetFocused,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&client_msg).unwrap();
        let _decoded = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();

        let service_msg = ServiceMessage::AgentQueryResult {
            request_id,
            result: AgentQueryResult::Focused(FocusedInfo {
                space: None,
                pane: None,
                tab: None,
            }),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&service_msg).unwrap();
        let _decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
    }
}
