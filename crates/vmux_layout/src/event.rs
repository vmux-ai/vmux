use vmux_core::{PageIcon, PageMetadata};

pub const LAYOUT_PAGE_URL: &str = "vmux://layout/";
pub const COMMAND_BAR_PAGE_URL: &str = "vmux://command-bar/";
pub const TERMINAL_PAGE_URL: &str = "vmux://terminal/";
pub const SERVICES_PAGE_URL: &str = "vmux://services/";
pub const LAYOUT_STATE_EVENT: &str = "layout-state";
pub const STACKS_EVENT: &str = "stacks";
pub const RELOAD_EVENT: &str = "reload";
/// Host -> layout page: open the command bar panel with a fresh payload. Distinct from
/// `COMMAND_BAR_OPEN_EVENT` so the layout page and the start page can be addressed separately.
pub const LAYOUT_COMMAND_BAR_OPEN_EVENT: &str = "layout-command-bar-open";

/// Host -> layout page: close the command bar panel.
///
/// The host cannot close the panel by mutating a surface the way the modal allowed, so dismiss
/// and the `Cmd+K`-while-open toggle both have to ask the page to unmount it.
pub const LAYOUT_COMMAND_BAR_CLOSE_EVENT: &str = "layout-command-bar-close";

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
pub struct CommandBarPanelCloseEvent;

/// Where the user has dragged and sized the floating command bar, in CSS pixels.
///
/// Absent until the first drag or resize, so an untouched bar keeps its centred default and
/// content-driven height.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PanelPlacement {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

pub const PANEL_MIN_WIDTH: f64 = 320.0;
pub const PANEL_MIN_HEIGHT: f64 = 120.0;

/// Keep the bar inside the window and above a usable minimum.
///
/// Without the clamp a drag can park the bar past the edge, where it is unreachable and there is
/// no chrome to drag it back by.
pub fn clamp_panel_placement(
    placement: PanelPlacement,
    viewport_width: f64,
    viewport_height: f64,
) -> PanelPlacement {
    let width = placement
        .width
        .clamp(PANEL_MIN_WIDTH, viewport_width.max(PANEL_MIN_WIDTH));
    let height = placement
        .height
        .clamp(PANEL_MIN_HEIGHT, viewport_height.max(PANEL_MIN_HEIGHT));
    PanelPlacement {
        left: placement.left.clamp(0.0, (viewport_width - width).max(0.0)),
        top: placement
            .top
            .clamp(0.0, (viewport_height - height).max(0.0)),
        width,
        height,
    }
}

/// Layout page -> host: the command bar panel took or released the keyboard.
///
/// Mirrors `BookmarkTextInputEvent`: while the panel holds a focused DOM field the layout shell
/// must own `CefKeyboardTarget`, or keystrokes go to the focused pane instead.
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
pub struct CommandBarPanelActiveEvent {
    pub active: bool,
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

    /// Left padding on the tab row to keep tab pills clear of the macOS
    /// traffic lights. Only needed when the side sheet is closed (when
    /// open, the side sheet covers the traffic-lights region).
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

/// Left padding (px) reserved on the tab row for the macOS traffic
/// lights when the side sheet is closed.
pub const TRAFFIC_LIGHTS_PAD_PX: f32 = 80.0;

/// Vertical space the CEF shell reserves in the layout above the pane.
/// The CEF shell puts tabs at the very top (traffic lights sit on the
/// left of the tab row, in the reserved padding) so no extra titlebar
/// strip is needed.
pub const CEF_RESERVED_HEIGHT_PX: f32 = HEADER_HEIGHT_PX;

/// Default window edge padding (px). Overridable via `settings.layout.window.padding`.
pub const WINDOW_PAD_PX: f32 = 8.0;

/// Default page bg color for terminal-like stacks (terminals, processes,
/// agent CLIs). Matches catppuccin-mocha `base` so the CEF URL row
/// blends with the terminal surface below it.
pub const TERMINAL_CEF_BG_COLOR: &str = "#1e1e2e";

/// Gap (px) between split panes inside a tab.
pub const PANE_GAP_PX: f32 = 4.0;

/// Default side-sheet width (px).
pub const SIDE_SHEET_WIDTH_PX: f32 = 220.0;

#[cfg(test)]
#[path = "event.test.rs"]
mod tests;
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

/// The active tab's working directory + live git status, auto-detected from git. Rendered as the
/// git-integration section of the side-sheet's first card.
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
    /// The effective dir is inside a git repository.
    pub is_git_repo: bool,
    pub is_worktree: bool,
    pub branch: String,
    pub base_ref: String,
    /// Uncommitted working-tree changes.
    pub uncommitted: u32,
    /// Commits ahead of upstream.
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
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct DebugUpdateReady {
    pub version: String,
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
pub struct DebugUpdateClear;

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
pub struct DebugSimulateDownload;

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

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct BookmarksCommandEvent {
    pub command: String,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub metadata: Option<PageMetadata>,
    #[serde(default)]
    pub folder: Option<String>,
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
#[path = "event.update_event.test.rs"]
mod update_event_tests;
