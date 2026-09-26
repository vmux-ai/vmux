use std::path::Path;

use vmux_core::PageIcon;

pub const LAYOUT_PAGE_URL: &str = "vmux://layout/";
pub const TERMINAL_PAGE_URL: &str = "vmux://terminal/";
#[vmux_api::contract(Default, Eq)]
pub struct ReloadEffect {
    pub revision: u64,
}
#[vmux_api::contract(Copy, Default)]
pub struct LayoutGeometry {
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

impl LayoutGeometry {
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
pub const SIDE_SHEET_MIN_WIDTH_PX: f32 = 160.0;
pub const SIDE_SHEET_MAX_WIDTH_PX: f32 = 640.0;

#[vmux_api::ui_event(Copy, target = "layout")]
pub struct SideSheetResizeEvent {
    pub width: f32,
    pub settled: bool,
}

impl SideSheetResizeEvent {
    pub fn live(width: f32) -> Self {
        Self {
            width,
            settled: false,
        }
    }

    pub fn settled(width: f32) -> Self {
        Self {
            width,
            settled: true,
        }
    }

    pub fn clamped(self) -> f32 {
        if !self.width.is_finite() {
            return SIDE_SHEET_WIDTH_PX;
        }
        self.width
            .clamp(SIDE_SHEET_MIN_WIDTH_PX, SIDE_SHEET_MAX_WIDTH_PX)
    }
}

#[vmux_api::ui_event(Default, target = "layout")]
pub struct WindowDragRegionEvent {
    pub id: String,
    #[serde(default)]
    pub removed: bool,
    #[serde(default)]
    pub blocked: bool,
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl WindowDragRegionEvent {
    pub fn is_finite(&self) -> bool {
        self.left.is_finite()
            && self.top.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
    }
}

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
        let open = LayoutGeometry {
            side_sheet_open: true,
            side_sheet_width: 280.0,
            pane_gap: 8.0,
            ..Default::default()
        };
        let closed = LayoutGeometry {
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
        let closed = LayoutGeometry {
            side_sheet_open: false,
            window_pad_left: 16.0,
            ..Default::default()
        };
        let open = LayoutGeometry {
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
        let state = LayoutGeometry {
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
        let closed = LayoutGeometry {
            side_sheet_open: false,
            ..Default::default()
        };
        let open = LayoutGeometry {
            side_sheet_open: true,
            ..Default::default()
        };

        assert_eq!(closed.tab_row_pad_left(), TRAFFIC_LIGHTS_PAD_PX);
        assert!(open.tab_row_pad_left() < TRAFFIC_LIGHTS_PAD_PX);
    }

    #[test]
    fn header_visibility_tracks_header_open() {
        let open = LayoutGeometry {
            header_open: true,
            side_sheet_open: false,
            ..Default::default()
        };
        let closed = LayoutGeometry {
            header_open: false,
            side_sheet_open: true,
            ..Default::default()
        };

        assert!(open.header_visible());
        assert!(!closed.header_visible());
    }

    #[test]
    fn tab_drop_placement_stays_relative_when_the_source_is_removed() {
        assert_eq!(TabDropPlacement::Before.destination(0, 2, 3), 1);
        assert_eq!(TabDropPlacement::After.destination(2, 0, 3), 1);
        assert_eq!(TabDropPlacement::Before.destination(2, 0, 3), 0);
        assert_eq!(TabDropPlacement::After.destination(0, 2, 3), 2);
    }
}
#[vmux_api::ui_event(Copy, Eq, target = "layout")]
pub struct HeaderBackRequest;

#[vmux_api::ui_event(Copy, Eq, target = "layout")]
pub struct HeaderForwardRequest;

#[vmux_api::ui_event(Copy, Eq, target = "layout")]
pub struct HeaderReloadRequest;

#[vmux_api::ui_event(Copy, Eq, target = "layout")]
pub struct HeaderAddressFocusRequest;

#[vmux_api::contract(Default, Eq)]
pub struct StackNavigationState {
    pub stacks: Vec<StackRow>,
    #[serde(default)]
    pub can_go_back: bool,
    #[serde(default)]
    pub can_go_forward: bool,
    #[serde(default)]
    pub is_zoomed: bool,
}

#[vmux_api::contract(Eq)]
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

#[vmux_api::contract(Default, Eq)]
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

#[vmux_api::contract(Default, Eq)]
pub struct TabListState {
    pub tabs: Vec<TabRow>,
}

#[vmux_api::contract(Eq)]
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

#[vmux_api::ui_event(Eq, target = "layout")]
pub struct TabCreateRequest;

#[vmux_api::ui_event(Eq, target = "layout")]
pub struct TabCloseRequest {
    pub tab_id: Option<String>,
}

#[vmux_api::ui_event(Eq, target = "layout")]
pub struct TabActivateRequest {
    pub tab_id: String,
}

#[vmux_api::ui_event(Eq, target = "layout")]
pub struct TabReorderRequest {
    pub tab_id: String,
    pub target_tab_id: String,
    pub drop_placement: TabDropPlacement,
}

#[vmux_api::contract(Copy, Eq)]
pub enum TabDropPlacement {
    Before,
    After,
}

impl TabDropPlacement {
    pub fn destination(self, from: usize, target: usize, len: usize) -> usize {
        let destination = match self {
            Self::Before if from < target => target.saturating_sub(1),
            Self::Before => target,
            Self::After if from < target => target,
            Self::After => target.saturating_add(1),
        };
        destination.min(len.saturating_sub(1))
    }
}

#[vmux_api::contract(Default)]
pub struct PaneTreeState {
    pub panes: Vec<PaneNode>,
}

#[vmux_api::contract(Copy, Eq)]
pub struct StackRevealTarget {
    pub pane_id: u64,
    pub stack_id: u64,
}

#[vmux_api::contract(Default)]
pub struct SideSheetState {
    pub active_space: Option<vmux_core::event::space::SpaceRow>,
    pub active_pane: Option<PaneNode>,
    pub active_page: Option<StackNode>,
    pub reveal: Option<StackRevealTarget>,
}

#[vmux_api::contract]
pub struct PaneNode {
    pub id: u64,
    pub is_active: bool,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub bookmarks_expanded: bool,
    pub stacks: Vec<StackNode>,
}

#[vmux_api::contract]
pub struct StackNode {
    pub id: u64,
    #[serde(default)]
    pub agent_id: Option<String>,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub icon: PageIcon,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_loading: bool,
    #[serde(default)]
    pub is_dirty: bool,
    #[serde(default)]
    pub bg_color: Option<String>,
}

#[vmux_api::ui_event(Eq, target = "layout")]
pub struct SideSheetStackActivateRequest {
    pub pane_id: u64,
    pub stack_id: u64,
}

#[vmux_api::ui_event(Eq, target = "layout")]
pub struct SideSheetStackCloseRequest {
    pub pane_id: u64,
    pub stack_id: u64,
}

#[vmux_api::ui_event(Eq, target = "layout")]
pub struct SideSheetStackCreateRequest {
    pub pane_id: u64,
}

#[vmux_api::ui_event(Eq, target = "layout")]
pub struct SideSheetProjectOpenRequest {
    pub pane_id: u64,
    pub path: String,
}

#[vmux_api::ui_event(Eq, target = "layout")]
pub struct SideSheetSectionRequest {
    pub pane_id: u64,
    pub path: String,
    pub expanded: bool,
}

impl SideSheetSectionRequest {
    pub fn new(pane_id: u64, path: impl Into<String>, expanded: bool) -> Self {
        Self {
            pane_id,
            path: path.into(),
            expanded,
        }
    }
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum RemotePhase {
    #[default]
    Disabled,
    Starting,
    Enabled,
    Error,
}

#[vmux_api::contract(Default, Eq)]
pub struct RemoteUiState {
    pub enabled: bool,
    pub phase: RemotePhase,
    pub pairing_url: String,
    pub pairing_deep_link: String,
    pub paired: bool,
    #[serde(default)]
    pub pairing_visible: bool,
    pub devices: Vec<RemoteDevice>,
    pub error: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct RemoteDevice {
    pub id: String,
}

#[vmux_api::ui_event(Copy, Default, Eq, target = "layout")]
pub struct RemoteRequest {
    pub enabled: bool,
}

#[vmux_api::ui_event(Copy, Eq, target = "layout")]
pub struct RemotePairingShowRequest;

#[vmux_api::ui_event(Copy, Eq, target = "layout")]
pub struct RemotePairingDismissRequest;

#[vmux_api::ui_event(Default, Eq, target = "layout")]
pub struct LayoutOverlayEvent {
    pub id: String,
    pub active: bool,
}

#[vmux_api::ui_event(Copy, Default, Eq, target = "layout")]
pub struct RemoteCopyEvent;

#[vmux_api::ui_event(Default, Eq, target = "layout")]
pub struct RemoteRevokeRequest {
    pub client_id: String,
}

#[vmux_api::contract(Default)]
pub struct TabBoundary {
    pub effective_dir: String,
    pub source: String,
    pub repository: String,
    pub is_git_repo: bool,
    pub is_worktree: bool,
    pub branch: String,
    pub base_ref: String,
    pub uncommitted: u32,
    pub ahead: u32,
    pub changed_files: u32,
    pub insertions: u32,
    pub deletions: u32,
    pub pane_count: u32,
}

#[vmux_api::contract(Default)]
pub struct TabBoundaryState {
    pub boundary: Option<TabBoundary>,
    pub projects: Vec<vmux_core::event::ProjectRow>,
}

#[vmux_api::contract(Default)]
pub struct ActiveSessionState {
    pub session: Option<ActiveSession>,
}

#[vmux_api::contract]
pub struct ActiveSession {
    pub page: StackNode,
    pub agent: Option<vmux_core::event::team::TeamMemberRow>,
    pub project: Option<ActiveWorkspaceProject>,
    pub boundary: Option<TabBoundary>,
    pub pane_id: u64,
}

#[vmux_api::contract]
pub struct ActiveWorkspaceProject {
    pub root: vmux_core::event::ProjectRow,
    pub children: Vec<vmux_core::event::ProjectRow>,
    pub choices: Vec<vmux_core::event::ProjectRow>,
}

#[vmux_api::contract(Default)]
pub struct HeaderPageState {
    pub active: Option<StackRow>,
    pub bookmarked: bool,
    pub pinned_uuid: Option<String>,
}

#[vmux_api::contract(Copy, Eq)]
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

#[vmux_api::contract(Default)]
pub struct UpdateReady {
    pub version: String,
}

#[vmux_api::contract(Default)]
pub struct UpdateProgress {
    pub version: String,
    pub downloaded: u64,
    pub total: u64,
    pub installing: bool,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub struct UpdateCleared;

#[vmux_api::ui_event(Copy, Default, Eq, targets = ["debug", "extensions", "layout"])]
pub struct RestartRequestEvent;

#[cfg(test)]
mod update_event_tests {
    use super::*;

    #[test]
    fn update_ready_event_rkyv_round_trips() {
        let evt = UpdateReady {
            version: "v9.9.9".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&evt).unwrap();
        let back = rkyv::from_bytes::<UpdateReady, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.version, "v9.9.9");
    }

    #[test]
    fn update_progress_event_rkyv_round_trips() {
        let evt = UpdateProgress {
            version: "0.0.20".to_string(),
            downloaded: 42,
            total: 100,
            installing: false,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&evt).unwrap();
        let back = rkyv::from_bytes::<UpdateProgress, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.version, "0.0.20");
        assert_eq!(back.downloaded, 42);
        assert_eq!(back.total, 100);
        assert!(!back.installing);
    }
}
