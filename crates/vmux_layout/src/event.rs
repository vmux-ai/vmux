pub const LAYOUT_PAGE_URL: &str = "vmux://layout/";
pub const COMMAND_BAR_PAGE_URL: &str = "vmux://command-bar/";
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
pub const PANE_TREE_EVENT: &str = "pane-tree";
pub const SIDE_SHEET_COMMAND_EVENT: &str = "side-sheet-command";
pub const SIDE_SHEET_DRAG_EVENT: &str = "side-sheet-drag";

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

pub const HEADER_HEIGHT_PX: f32 = 72.0;
pub const SPACES_ROW_HEIGHT_PX: f32 = 28.0;

/// Left padding (px) reserved on the tab row for the macOS traffic
/// lights when the side sheet is closed.
pub const TRAFFIC_LIGHTS_PAD_PX: f32 = 80.0;

/// Vertical space the CEF shell reserves in the layout above the pane.
/// The CEF shell puts tabs at the very top (traffic lights sit on the
/// left of the tab row, in the reserved padding) so no extra titlebar
/// strip is needed.
pub const CEF_RESERVED_HEIGHT_PX: f32 = HEADER_HEIGHT_PX;

/// Hardcoded window edge padding (px). Not user-configurable.
pub const WINDOW_PAD_PX: f32 = 4.0;

/// Default page bg color for terminal-like stacks (terminals, processes,
/// agent CLIs). Matches catppuccin-mocha `base` so the CEF URL row
/// blends with the terminal surface below it.
pub const TERMINAL_CEF_BG_COLOR: &str = "#1e1e2e";

/// Gap (px) between split panes inside a tab.
pub const PANE_GAP_PX: f32 = 4.0;

/// Default side-sheet width (px).
pub const SIDE_SHEET_WIDTH_PX: f32 = 220.0;

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
    pub favicon_url: String,
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
    pub favicon_url: String,
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
    pub favicon_url: String,
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
