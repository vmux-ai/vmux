use super::SimulatorInput;
use crate::{ProcessId, json::JsonValue};

#[vmux_api::contract(Eq)]
pub struct AgentSpace {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub is_active: bool,
}

#[vmux_api::contract(Eq)]
pub struct AgentBookmark {
    pub uuid: String,
    pub url: String,
    pub title: String,
    pub favicon_url: String,
}

impl AgentBookmark {
    pub fn new(
        uuid: impl Into<String>,
        url: impl Into<String>,
        title: impl Into<String>,
        favicon_url: impl Into<String>,
    ) -> Self {
        Self {
            uuid: uuid.into(),
            url: url.into(),
            title: title.into(),
            favicon_url: favicon_url.into(),
        }
    }
}

#[vmux_api::contract(Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentBookmarkNode {
    Entry {
        #[serde(flatten)]
        bookmark: AgentBookmark,
    },
    Folder {
        uuid: String,
        name: String,
        collapsed: bool,
        children: Vec<AgentBookmark>,
    },
}

#[vmux_api::contract(Default, Eq)]
pub struct AgentBookmarks {
    pub pins: Vec<AgentBookmark>,
    pub roots: Vec<AgentBookmarkNode>,
}

#[vmux_api::contract(Eq)]
pub struct AgentCommandTool {
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQuery {
    ReadLayout {
        anchor: Option<ProcessId>,
    },
    ReadProcessOutput {
        process_id: ProcessId,
    },
    ReadProcessTranscript {
        process_id: ProcessId,
    },
    ProcessCommandExit {
        process_id: ProcessId,
    },
    ProcessRunCompletion {
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
        input: SimulatorInput,
    },
    WorkingDirectory {
        anchor: ProcessId,
    },
    VaultStatus,
    ListCommands,
}

#[vmux_api::contract(Eq)]
pub struct AgentCommandExit {
    pub sequence: u64,
    pub exit: Option<i32>,
}

#[vmux_api::contract(Eq)]
pub struct AgentRunCompletion {
    pub token: Option<String>,
    pub exit: Option<i32>,
}

#[vmux_api::contract(Eq)]
pub struct AgentImage {
    pub path: String,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[vmux_api::contract(Eq)]
pub struct AgentRecording {
    pub mp4_path: String,
    pub gif_path: Option<String>,
    pub duration_ms: u64,
    pub bytes: u64,
    pub auto_stopped: bool,
}
