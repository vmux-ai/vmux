use super::SimulatorAction;
use crate::ProcessId;

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQuery {
    ReadLayout {
        anchor: Option<ProcessId>,
    },
    ReadTerminal {
        process_id: ProcessId,
    },
    ReadTerminalFull {
        process_id: ProcessId,
    },
    CommandExit {
        process_id: ProcessId,
    },
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
    SimulatorScreenshot,
    SimulatorControl {
        action: SimulatorAction,
    },
    WorkingDirectory {
        anchor: ProcessId,
    },
    VaultStatus,
}

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQueryResult {
    Layout(crate::protocol::layout::LayoutSnapshot),
    VaultStatus(crate::vault::VaultStatusSnapshot),
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
