use serde::{Deserialize, Serialize};

pub const EXPLORER_TREE_EVENT: &str = "explorer_tree";
pub const EXPLORER_FOCUS_EVENT: &str = "explorer_focus";
pub const EXPLORER_OPEN_EDITORS_EVENT: &str = "explorer_open_editors";
pub const EXPLORER_OUTLINE_EVENT: &str = "explorer_outline";
pub const EXPLORER_CHROME_EVENT: &str = "explorer_chrome";
pub const EXPLORER_FS_RESULT_EVENT: &str = "explorer_fs_result";
pub const EXPLORER_TREE_TOGGLE_EVENT: &str = "explorer_tree_toggle";
pub const EXPLORER_TREE_PREFETCH_EVENT: &str = "explorer_tree_prefetch";
pub const EXPLORER_TREE_REFRESH_EVENT: &str = "explorer_tree_refresh";
pub const EXPLORER_REVEAL_CURRENT_EVENT: &str = "explorer_reveal_current";
pub const EXPLORER_CREATE_EVENT: &str = "explorer_create";
pub const EXPLORER_RENAME_EVENT: &str = "explorer_rename";
pub const EXPLORER_DELETE_EVENT: &str = "explorer_delete";
pub const EXPLORER_CLOSE_EDITOR_EVENT: &str = "explorer_close_editor";
pub const EXPLORER_PANEL_TOGGLE_EVENT: &str = "explorer_panel_toggle";
pub const EXPLORER_PANEL_WIDTH_EVENT: &str = "explorer_panel_width";
pub const EXPLORER_GOTO_EVENT: &str = "explorer_goto";
pub const EXPLORER_SEARCH_EVENT: &str = "explorer_search";
pub const EXPLORER_SEARCH_OPEN_EVENT: &str = "explorer_search_open";

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
pub struct ExplorerChromeEvent {
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
)]
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
)]
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
)]
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
)]
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
)]
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
)]
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
)]
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
)]
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
)]
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
)]
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
)]
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
)]
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
)]
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
)]
pub struct ExplorerSearchRequest {
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}
