#[vmux_api::contract(Eq)]
pub struct TreeRow {
    pub name: String,
    pub path: String,
    pub depth: u16,
    pub is_dir: bool,
    pub expanded: bool,
    pub loading: bool,
}

#[vmux_api::contract(Eq)]
pub struct OpenEditorItem {
    pub name: String,
    pub path: String,
    pub active: bool,
    pub dirty: bool,
    pub is_dir: bool,
}

#[vmux_api::contract(Eq)]
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

#[vmux_api::contract(Eq)]
pub struct ExplorerTreeEvent {
    pub root_name: String,
    pub root_path: String,
    pub current_path: String,
    pub focus_path: String,
    pub loading: bool,
    pub rows: Vec<TreeRow>,
}

#[vmux_api::contract(Eq)]
pub struct ExplorerFocusEvent {
    pub path: String,
    pub reveal: ExplorerReveal,
}

#[vmux_api::contract(Copy, Eq)]
pub enum ExplorerReveal {
    Requested,
    Followed,
}

#[vmux_api::contract(Eq)]
pub struct ExplorerFsResult {
    pub ok: bool,
    pub message: String,
    pub open_path: String,
}

#[vmux_api::contract(Eq)]
pub struct OpenEditorsEvent {
    pub items: Vec<OpenEditorItem>,
}

#[vmux_api::contract(Eq)]
pub struct OutlineEvent {
    pub items: Vec<OutlineRow>,
}

#[vmux_api::contract(Copy, Eq)]
pub struct ExplorerPanelEvent {
    pub visible: bool,
    pub width: u32,
    pub client_id: u64,
    pub request_id: u64,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct ExplorerTreeToggle {
    pub path: String,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct ExplorerTreePrefetch {
    pub path: String,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct ExplorerTreeRefresh {
    pub path: String,
}

#[vmux_api::ui_event(Default, Eq, target = "files")]
pub struct ExplorerRevealCurrent;

#[vmux_api::ui_event(Copy, Default, Eq, target = "files")]
pub struct ExplorerCollapseAll;

#[vmux_api::ui_event(Eq, target = "files")]
pub struct ExplorerCreate {
    pub parent: String,
    pub name: String,
    pub is_dir: bool,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct ExplorerRename {
    pub path: String,
    pub name: String,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct ExplorerDelete {
    pub path: String,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct ExplorerCloseEditor {
    pub path: String,
}

#[vmux_api::ui_event(Copy, Default, Eq, target = "files")]
pub struct ExplorerPanelSetVisible {
    pub visible: bool,
    pub client_id: u64,
    pub request_id: u64,
}

#[vmux_api::ui_event(Copy, Eq, target = "files")]
pub struct ExplorerPanelWidth {
    pub px: u32,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct ExplorerGoto {
    pub path: String,
    pub line: u32,
}

#[vmux_api::contract(Eq)]
pub struct ExplorerSearchMatch {
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub preview: String,
}

#[vmux_api::contract(Eq)]
pub struct ExplorerSearchFile {
    pub path: String,
    pub matches: Vec<ExplorerSearchMatch>,
    pub capped: bool,
}

#[vmux_api::contract(Default, Eq)]
pub struct ExplorerSearchEvent {
    pub root: String,
    pub query: String,
    pub files: Vec<ExplorerSearchFile>,
    pub capped: bool,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct ExplorerSearchOpen {
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
}

#[vmux_api::ui_event(Default, Eq, target = "files")]
pub struct ExplorerSearchRequest {
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}
