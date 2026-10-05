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
    pub context: String,
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
    pub loading: bool,
    pub rows: Vec<TreeRow>,
}

#[vmux_api::contract(Eq)]
pub struct ExplorerFocusEvent {
    pub revision: u64,
    pub path: String,
    pub reveal: ExplorerReveal,
}

#[vmux_api::contract(Copy, Eq)]
pub enum ExplorerReveal {
    Requested,
    Followed,
}

#[vmux_api::contract(Default, Eq)]
pub struct ExplorerNotice {
    pub ok: bool,
    pub message: Option<String>,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerNoticeDismissRequest;

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
    pub search: bool,
    pub open_editors: bool,
    pub files: bool,
    pub outline: bool,
    pub search_focus_revision: u64,
}

#[vmux_api::contract(Default, Eq)]
pub struct ExplorerPromptState {
    pub open: bool,
    pub title_message_id: String,
    pub name: String,
    pub draft: String,
    pub destructive: bool,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerPanelViewSet {
    pub search: bool,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerOpenEditorsToggle;

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerFilesToggle;

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerOutlineToggle;

#[vmux_api::ui_event(Eq)]
pub struct ExplorerTreeToggle {
    pub path: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ExplorerTreePrefetch {
    pub path: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ExplorerTreeRefresh {
    pub path: String,
}

#[vmux_api::ui_event]
pub struct ExplorerRevealCurrent;

#[vmux_api::ui_event]
pub struct ExplorerCollapseAll;

#[vmux_api::ui_event(Eq)]
pub struct ExplorerCreateFilePromptRequest {
    pub path: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ExplorerCreateDirectoryPromptRequest {
    pub path: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ExplorerRenamePromptRequest {
    pub path: String,
    pub name: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ExplorerDeletePromptRequest {
    pub path: String,
    pub name: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ExplorerPromptDraftRequest {
    pub draft: String,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerPromptSubmitRequest;

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerPromptDismissRequest;

#[vmux_api::ui_event(Eq)]
pub struct ExplorerCloseEditor {
    pub path: String,
}

#[vmux_api::ui_event(Copy, Default, Eq)]
pub struct ExplorerPanelSetVisible {
    pub visible: bool,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerPanelWidth {
    pub px: u32,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerPanelViewportWidth {
    pub px: u32,
}

#[vmux_api::ui_event(Eq)]
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

impl ExplorerSearchMatch {
    pub fn key(&self, path: &str) -> String {
        Self::key_at(path, self.line, self.col)
    }

    pub fn key_at(path: &str, line: u32, col: u32) -> String {
        format!("{path}:{line}:{col}")
    }
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
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub files: Vec<ExplorerSearchFile>,
    pub capped: bool,
    pub collapsed: Vec<String>,
    pub opened: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ExplorerSearchOpen {
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
}

#[vmux_api::ui_event(Eq)]
pub struct ExplorerSearchGroupToggle {
    pub path: String,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerSearchCollapseAll;

#[vmux_api::ui_event(Copy, Eq)]
pub struct ExplorerSearchClear;

#[vmux_api::ui_event(Default, Eq)]
pub struct ExplorerSearchDraftRequest {
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct ExplorerSearchRequest {
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}
