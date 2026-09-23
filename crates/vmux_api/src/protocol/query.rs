use super::SimulatorAction;
use crate::{ProcessId, json::JsonValue};

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
pub struct AgentSpace {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub is_active: bool,
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

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentBookmarks {
    pub pins: Vec<AgentBookmark>,
    pub roots: Vec<AgentBookmarkNode>,
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
    ListCommands,
}

#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQueryResult {
    Layout(crate::protocol::layout::LayoutSnapshot),
    VaultStatus(crate::vault::VaultStatusSnapshot),
    Text(String),
    Settings(JsonValue),
    Spaces(Vec<AgentSpace>),
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
    Bookmarks(AgentBookmarks),
    Commands(Vec<AgentCommandTool>),
}
