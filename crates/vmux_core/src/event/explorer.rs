use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TreeRow {
    pub name: String,
    pub path: String,
    pub depth: u16,
    pub is_dir: bool,
    pub expanded: bool,
    pub loading: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct OpenEditorItem {
    pub name: String,
    pub path: String,
    pub active: bool,
    pub dirty: bool,
    pub is_dir: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct OutlineRow {
    pub name: String,
    pub kind: u8,
    pub line: u32,
    pub end_line: u32,
    pub depth: u16,
}

impl OutlineRow {
    pub const OPEN_END: u32 = u32::MAX;

    pub fn contains(&self, line: u32) -> bool {
        self.line <= line && line <= self.end_line
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ExplorerTreeEvent {
    pub root_name: String,
    pub root_path: String,
    pub current_path: String,
    pub focus_path: String,
    pub loading: bool,
    pub rows: Vec<TreeRow>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ExplorerFocusEvent {
    pub path: String,
    pub reveal: ExplorerReveal,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum ExplorerReveal {
    Requested,
    Followed,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ExplorerFsResult {
    pub ok: bool,
    pub message: String,
    pub open_path: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct OpenEditorsEvent {
    pub items: Vec<OpenEditorItem>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct OutlineEvent {
    pub items: Vec<OutlineRow>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ExplorerPanelEvent {
    pub visible: bool,
    pub width: u32,
    pub client_id: u64,
    pub request_id: u64,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerTreeToggle {
    pub path: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerTreePrefetch {
    pub path: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerTreeRefresh {
    pub path: String,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerRevealCurrent;

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerCollapseAll;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerCreate {
    pub parent: String,
    pub name: String,
    pub is_dir: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerRename {
    pub path: String,
    pub name: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerDelete {
    pub path: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerCloseEditor {
    pub path: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerPanelSetVisible {
    pub visible: bool,
    pub client_id: u64,
    pub request_id: u64,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerPanelWidth {
    pub px: u32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerGoto {
    pub path: String,
    pub line: u32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ExplorerSearchMatch {
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub preview: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ExplorerSearchFile {
    pub path: String,
    pub matches: Vec<ExplorerSearchMatch>,
    pub capped: bool,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ExplorerSearchEvent {
    pub root: String,
    pub query: String,
    pub files: Vec<ExplorerSearchFile>,
    pub capped: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerSearchOpen {
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(target = "files")]
pub struct ExplorerSearchRequest {
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}
