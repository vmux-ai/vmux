use std::path::Path;

use vmux_core::{PageIcon, PageMetadata};

pub const LAYOUT_PAGE_URL: &str = "vmux://layout/";
pub const TERMINAL_PAGE_URL: &str = "vmux://terminal/";
pub const SERVICES_PAGE_URL: &str = "vmux://services/";
pub const LAYOUT_STATE_EVENT: &str = "layout-state";
pub const STACKS_EVENT: &str = "stacks";
pub const RELOAD_EVENT: &str = "reload";
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ReloadEvent;
pub const TABS_EVENT: &str = "tabs";
pub const BOOKMARKS_EVENT: &str = "bookmarks";
pub const PANE_TREE_EVENT: &str = "pane-tree";
pub const SIDE_SHEET_COMMAND_EVENT: &str = "side-sheet-command";
pub const SIDE_SHEET_DRAG_EVENT: &str = "side-sheet-drag";
pub const TAB_BOUNDARY_EVENT: &str = "tab-boundary";
pub const REMOTE_STATE_EVENT: &str = "remote-state";
pub const REMOTE_COMMAND_EVENT: &str = "remote-command";

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct LayoutStateEvent {
    #[serde(default)]
    pub header_open: bool,
    #[serde(default)]
    pub side_sheet_open: bool,
    #[serde(default = "default_header_height")]
    pub header_height: f32,
    #[serde(default = "default_side_sheet_width")]
    pub side_sheet_width: f32,
    #[serde(default = "default_pane_gap")]
    pub pane_gap: f32,
    #[serde(default)]
    pub radius: f32,
    #[serde(default)]
    pub header_left: Option<f32>,
    #[serde(default)]
    pub header_top: Option<f32>,
    #[serde(default)]
    pub header_right: Option<f32>,
    #[serde(default)]
    pub window_pad_top: f32,
    #[serde(default = "default_window_pad")]
    pub window_pad_right: f32,
    #[serde(default = "default_window_pad")]
    pub window_pad_bottom: f32,
    #[serde(default)]
    pub window_pad_left: f32,
}

impl LayoutStateEvent {
    pub fn main_cef_left(&self) -> f32 {
        if self.side_sheet_open {
            self.window_pad_left + self.side_sheet_width + self.pane_gap
        } else {
            self.window_pad_left
        }
    }

    pub fn header_left(&self) -> f32 {
        self.header_left.unwrap_or_else(|| self.main_cef_left())
    }

    pub fn header_top(&self) -> f32 {
        self.header_top.unwrap_or(self.window_pad_top)
    }

    pub fn header_right(&self) -> f32 {
        self.header_right.unwrap_or(self.window_pad_right)
    }

    pub fn header_visible(&self) -> bool {
        self.header_open
    }

    pub fn tab_row_pad_left(&self) -> f32 {
        if self.side_sheet_open {
            8.0
        } else {
            TRAFFIC_LIGHTS_PAD_PX
        }
    }
}

pub fn url_bar_top() -> f32 {
    SPACES_ROW_HEIGHT_PX
}

fn default_header_height() -> f32 {
    HEADER_HEIGHT_PX
}

fn default_side_sheet_width() -> f32 {
    280.0
}

fn default_pane_gap() -> f32 {
    8.0
}

fn default_window_pad() -> f32 {
    WINDOW_PAD_PX
}

pub const HEADER_HEIGHT_PX: f32 = 84.0;
pub const SPACES_ROW_HEIGHT_PX: f32 = 28.0;

pub const TRAFFIC_LIGHTS_PAD_PX: f32 = 80.0;

pub const CEF_RESERVED_HEIGHT_PX: f32 = HEADER_HEIGHT_PX;

pub const WINDOW_PAD_PX: f32 = 8.0;

pub const TERMINAL_CEF_BG_COLOR: &str = "#1e1e2e";

pub const PANE_GAP_PX: f32 = 4.0;

pub const SIDE_SHEET_WIDTH_PX: f32 = 220.0;

#[cfg(test)]
mod address_tests {
    use super::*;

    #[test]
    fn a_web_address_is_its_host_and_the_page_name() {
        let parts = AddressParts::web("https://github.com/vmux-ai/vmux/pull/412", "Add a pill");
        assert_eq!(parts.origin, "github.com");
        assert_eq!(parts.rest, "Add a pill");
    }

    #[test]
    fn an_untitled_web_address_is_its_host_alone() {
        let parts = AddressParts::web("https://github.com/vmux-ai/vmux", "  ");
        assert_eq!(parts.origin, "github.com");
        assert_eq!(parts.rest, "");
    }

    #[test]
    fn an_address_with_no_host_stands_alone() {
        let parts = AddressParts::web("about:blank", "");
        assert_eq!(parts.origin, "");
        assert_eq!(parts.rest, "about:blank");
    }

    #[test]
    fn an_internal_page_is_shown_whole_and_unpilled() {
        let parts = AddressParts::internal("vmux://terminal/");
        assert_eq!(parts.origin, "");
        assert_eq!(parts.rest, "vmux://terminal");
    }

    #[test]
    fn a_file_in_a_checkout_is_shown_against_its_repository_and_branch() {
        let parts = AddressParts::in_repo(
            Path::new("/w/.worktrees/lsp/client/crates/app/main.rs"),
            Path::new("/w/.worktrees/lsp/client"),
            "vmux",
            "lsp-bidirectional",
        );
        assert_eq!(parts.origin, "vmux@lsp-bidirectional");
        assert_eq!(parts.rest, "crates/app/main.rs");
    }

    #[test]
    fn a_checkout_on_no_branch_is_shown_as_the_repository_alone() {
        let parts = AddressParts::in_repo(Path::new("/w/a.rs"), Path::new("/w"), "vmux", "");
        assert_eq!(parts.origin, "vmux");
        assert_eq!(parts.rest, "a.rs");
    }

    #[test]
    fn a_file_outside_any_checkout_is_shown_against_home() {
        let parts = AddressParts::on_disk(
            Path::new("/Users/me/Downloads/a.txt"),
            Path::new("/Users/me"),
        );
        assert_eq!(parts.origin, "~");
        assert_eq!(parts.rest, "Downloads/a.txt");
    }

    #[test]
    fn a_file_outside_home_is_shown_against_the_filesystem() {
        let parts = AddressParts::on_disk(Path::new("/usr/local/bin/vmux"), Path::new("/Users/me"));
        assert_eq!(parts.origin, "/");
        assert_eq!(parts.rest, "usr/local/bin/vmux");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_cef_left_includes_side_sheet_gap_when_open() {
        let open = LayoutStateEvent {
            side_sheet_open: true,
            side_sheet_width: 280.0,
            pane_gap: 8.0,
            ..Default::default()
        };
        let closed = LayoutStateEvent {
            side_sheet_open: false,
            side_sheet_width: 280.0,
            pane_gap: 8.0,
            ..Default::default()
        };

        assert_eq!(open.main_cef_left(), 288.0);
        assert_eq!(closed.main_cef_left(), 0.0);
    }

    #[test]
    fn main_cef_left_includes_effective_window_left_padding() {
        let closed = LayoutStateEvent {
            side_sheet_open: false,
            window_pad_left: 16.0,
            ..Default::default()
        };
        let open = LayoutStateEvent {
            side_sheet_open: true,
            side_sheet_width: 280.0,
            pane_gap: 8.0,
            window_pad_left: 16.0,
            ..Default::default()
        };

        assert_eq!(closed.main_cef_left(), 16.0);
        assert_eq!(open.main_cef_left(), 304.0);
    }

    #[test]
    fn header_offsets_can_override_derived_window_padding() {
        let state = LayoutStateEvent {
            side_sheet_open: true,
            side_sheet_width: 220.0,
            pane_gap: 4.0,
            window_pad_left: 8.0,
            window_pad_top: 2.0,
            window_pad_right: 8.0,
            header_left: Some(230.0),
            header_top: Some(1.0),
            header_right: Some(9.0),
            ..Default::default()
        };

        assert_eq!(state.main_cef_left(), 232.0);
        assert_eq!(state.header_left(), 230.0);
        assert_eq!(state.header_top(), 1.0);
        assert_eq!(state.header_right(), 9.0);
    }

    #[test]
    fn tab_row_pad_left_clears_traffic_lights_when_side_sheet_closed() {
        let closed = LayoutStateEvent {
            side_sheet_open: false,
            ..Default::default()
        };
        let open = LayoutStateEvent {
            side_sheet_open: true,
            ..Default::default()
        };

        assert_eq!(closed.tab_row_pad_left(), TRAFFIC_LIGHTS_PAD_PX);
        assert!(open.tab_row_pad_left() < TRAFFIC_LIGHTS_PAD_PX);
    }

    #[test]
    fn header_visibility_tracks_header_open() {
        let open = LayoutStateEvent {
            header_open: true,
            side_sheet_open: false,
            ..Default::default()
        };
        let closed = LayoutStateEvent {
            header_open: false,
            side_sheet_open: true,
            ..Default::default()
        };

        assert!(open.header_visible());
        assert!(!closed.header_visible());
    }

    #[test]
    fn header_command_event_rkyv_roundtrip() {
        let original = HeaderCommandEvent {
            header_command: "back".into(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let recovered =
            rkyv::from_bytes::<HeaderCommandEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(recovered.header_command, "back");
    }

    #[test]
    fn tabs_command_event_rkyv_roundtrip() {
        let original = TabsCommandEvent {
            command: "switch-tab".into(),
            tab_id: Some("work".into()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let recovered =
            rkyv::from_bytes::<TabsCommandEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(recovered.command, "switch-tab");
        assert_eq!(recovered.tab_id.as_deref(), Some("work"));
    }
}
#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HeaderCommandEvent {
    pub header_command: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StacksHostEvent {
    pub stacks: Vec<StackRow>,
    #[serde(default)]
    pub can_go_back: bool,
    #[serde(default)]
    pub can_go_forward: bool,
    #[serde(default)]
    pub is_zoomed: bool,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StackRow {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub icon: PageIcon,
    pub is_active: bool,
    #[serde(default)]
    pub bg_color: Option<String>,
    #[serde(default)]
    pub address: AddressParts,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AddressParts {
    pub origin: String,
    pub rest: String,
}

impl AddressParts {
    pub fn web(url: &str, title: &str) -> Self {
        let host = vmux_ui::favicon::host_for_favicon_fallback(url).unwrap_or_default();
        let title = title.trim();
        if title.is_empty() {
            return match host.is_empty() {
                true => Self::new("", url),
                false => Self::new(host, ""),
            };
        }
        Self::new(host, title)
    }

    pub fn internal(url: &str) -> Self {
        Self::new("", url.trim_end_matches('/'))
    }

    pub fn in_repo(path: &Path, root: &Path, name: &str, branch: &str) -> Self {
        let Ok(rest) = path.strip_prefix(root) else {
            return Self::new("", path.to_string_lossy());
        };
        let origin = match branch.is_empty() {
            true => name.to_string(),
            false => format!("{name}@{branch}"),
        };
        Self::new(origin, rest.to_string_lossy())
    }

    pub fn on_disk(path: &Path, home: &Path) -> Self {
        match path.strip_prefix(home) {
            Ok(rest) => Self::new("~", rest.to_string_lossy()),
            Err(_) => Self::new("/", path.to_string_lossy().trim_start_matches('/')),
        }
    }

    fn new(origin: impl Into<String>, rest: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            rest: rest.into(),
        }
    }
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TabsHostEvent {
    pub tabs: Vec<TabRow>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TabRow {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    #[serde(default)]
    pub bg_color: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub icon: PageIcon,
    #[serde(default)]
    pub is_done_unseen: bool,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TabsCommandEvent {
    pub command: String,
    #[serde(default)]
    pub tab_id: Option<String>,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PaneTreeEvent {
    pub panes: Vec<PaneNode>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PaneNode {
    pub id: u64,
    pub is_active: bool,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub projects_expanded: bool,
    #[serde(default)]
    pub bookmarks_expanded: bool,
    #[serde(default)]
    pub knowledge_expanded: bool,
    #[serde(default)]
    pub tools_expanded: bool,
    pub stacks: Vec<StackNode>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StackNode {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub icon: PageIcon,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub stack_index: u32,
    #[serde(default)]
    pub is_loading: bool,
    #[serde(default)]
    pub bg_color: Option<String>,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SideSheetCommandEvent {
    pub command: String,
    #[serde(default)]
    pub pane_id: String,
    #[serde(default)]
    pub stack_index: u32,
    #[serde(default)]
    pub path: String,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum RemotePhase {
    #[default]
    Disabled,
    Starting,
    Enabled,
    Error,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RemoteStateEvent {
    pub enabled: bool,
    pub phase: RemotePhase,
    pub pairing_url: String,
    pub pairing_deep_link: String,
    pub paired: bool,
    pub error: String,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RemoteCommandEvent {
    pub enabled: bool,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RemoteCopyEvent;

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TabBoundary {
    pub effective_dir: String,
    pub source: String,
    pub is_git_repo: bool,
    pub is_worktree: bool,
    pub branch: String,
    pub base_ref: String,
    pub uncommitted: u32,
    pub ahead: u32,
    pub pane_count: u32,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TabBoundaryEvent {
    pub boundary: Option<TabBoundary>,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum SplitDirection {
    Row,
    Column,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum LayoutNode {
    Split {
        id: u64,
        direction: SplitDirection,
        children: Vec<LayoutNode>,
        flex_weights: Vec<f32>,
    },
    Pane {
        id: u64,
        is_active: bool,
        stacks: Vec<StackNode>,
    },
}

pub const UPDATE_READY_EVENT: &str = "update-ready";
pub const UPDATE_CLEARED_EVENT: &str = "update-cleared";
pub const UPDATE_PROGRESS_EVENT: &str = "update-progress";

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct UpdateReadyEvent {
    pub version: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct UpdateProgressEvent {
    pub version: String,
    pub downloaded: u64,
    pub total: u64,
    pub installing: bool,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct UpdateClearedEvent;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RestartRequestEvent;

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct BookmarkRow {
    pub uuid: String,
    pub metadata: PageMetadata,
    pub bookmarked: bool,
    pub pinned: bool,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FolderRow {
    pub uuid: String,
    pub name: String,
    pub collapsed: bool,
    pub parent: Option<String>,
    pub children: Vec<BookmarkRow>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum BookmarkNode {
    Entry(BookmarkRow),
    Folder(FolderRow),
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct BookmarksHostEvent {
    pub pins: Vec<BookmarkRow>,
    pub roots: Vec<BookmarkNode>,
}

pub use vmux_command::event::BookmarksCommandEvent;

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct BookmarkTextInputEvent {
    pub active: bool,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct BookmarkContextMenuEvent {
    pub active: bool,
}

#[cfg(test)]
mod update_event_tests {
    use super::*;

    #[test]
    fn update_ready_event_rkyv_round_trips() {
        let evt = UpdateReadyEvent {
            version: "v9.9.9".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&evt).unwrap();
        let back = rkyv::from_bytes::<UpdateReadyEvent, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.version, "v9.9.9");
    }

    #[test]
    fn update_progress_event_rkyv_round_trips() {
        let evt = UpdateProgressEvent {
            version: "0.0.20".to_string(),
            downloaded: 42,
            total: 100,
            installing: false,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&evt).unwrap();
        let back = rkyv::from_bytes::<UpdateProgressEvent, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.version, "0.0.20");
        assert_eq!(back.downloaded, 42);
        assert_eq!(back.total, 100);
        assert!(!back.installing);
    }

    #[test]
    fn event_ids_are_stable() {
        assert_eq!(UPDATE_READY_EVENT, "update-ready");
        assert_eq!(UPDATE_CLEARED_EVENT, "update-cleared");
        assert_eq!(UPDATE_PROGRESS_EVENT, "update-progress");
    }
}
