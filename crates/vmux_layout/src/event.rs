pub const LAYOUT_WEBVIEW_URL: &str = "vmux://layout/";
pub const COMMAND_BAR_WEBVIEW_URL: &str = "vmux://command-bar/";
pub const TERMINAL_WEBVIEW_URL: &str = "vmux://terminal/";
pub const PROCESSES_WEBVIEW_URL: &str = "vmux://services/";
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
    #[serde(default = "default_titlebar_height")]
    pub titlebar_height: f32,
    #[serde(default)]
    pub radius: f32,
}

impl LayoutStateEvent {
    pub fn main_chrome_left(&self) -> f32 {
        if self.side_sheet_open {
            self.side_sheet_width + self.pane_gap
        } else {
            0.0
        }
    }

    pub fn header_height_total(&self) -> f32 {
        self.titlebar_height + self.header_height
    }

    pub fn header_visible(&self) -> bool {
        self.header_open
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

fn default_titlebar_height() -> f32 {
    44.0
}

pub fn effective_titlebar_height(configured_height: f32) -> f32 {
    configured_height.max(default_titlebar_height())
}

fn titlebar_nav_left_padding() -> f32 {
    92.0
}

pub fn titlebar_nav_style(titlebar_height: f32) -> String {
    format!(
        "height:{titlebar_height}px;padding-left:{}px;justify-content:flex-end;",
        titlebar_nav_left_padding()
    )
}

pub const HEADER_HEIGHT_PX: f32 = 60.0;
pub const SPACES_ROW_HEIGHT_PX: f32 = 28.0;

/// Total vertical space the chrome reserves in the layout above the pane:
/// the titlebar overlap area (where macOS draws the traffic lights) and
/// the URL row height. Bevy's Header entity uses this for its node
/// height so the pane begins exactly where the chrome webview's URL row
/// ends — anything less would leave the pane peeking behind the chrome.
pub const CHROME_RESERVED_HEIGHT_PX: f32 = 44.0 + HEADER_HEIGHT_PX;

/// Hardcoded window edge padding (px). Not user-configurable.
pub const WINDOW_PAD_PX: f32 = 4.0;

/// Default page bg color for terminal-like stacks (terminals, processes,
/// agent CLIs). Matches catppuccin-mocha `base` so the chrome url row
/// blends with the terminal surface below it.
pub const TERMINAL_CHROME_BG_COLOR: &str = "#1e1e2e";

/// Gap (px) between split panes inside a tab.
pub const PANE_GAP_PX: f32 = 4.0;

/// Default side-sheet width (px).
pub const SIDE_SHEET_WIDTH_PX: f32 = 220.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_chrome_left_includes_side_sheet_gap_when_open() {
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

        assert_eq!(open.main_chrome_left(), 288.0);
        assert_eq!(closed.main_chrome_left(), 0.0);
    }

    #[test]
    fn header_height_total_clears_titlebar_controls() {
        let state = LayoutStateEvent {
            header_height: 40.0,
            titlebar_height: 28.0,
            ..Default::default()
        };

        assert_eq!(state.header_height_total(), 68.0);
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
    fn titlebar_height_keeps_minimum_traffic_light_clearance() {
        assert_eq!(effective_titlebar_height(0.0), 44.0);
        assert_eq!(effective_titlebar_height(52.0), 52.0);
    }

    #[test]
    fn titlebar_nav_style_clears_lights_and_right_aligns_controls() {
        assert_eq!(
            titlebar_nav_style(44.0),
            "height:44px;padding-left:92px;justify-content:flex-end;"
        );
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
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PageBgColorEvent {
    pub color: String,
}

#[cfg_attr(not(target_arch = "wasm32"), derive(bevy_ecs::message::Message))]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum SideSheetDragCommand {
    MoveStack {
        from_pane: u64,
        from_index: usize,
        to_pane: u64,
        to_index: usize,
    },
    SwapPane {
        pane: u64,
        target: u64,
    },
    SplitPane {
        dragged: u64,
        target: u64,
        edge: Edge,
    },
}
