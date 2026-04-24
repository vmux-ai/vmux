pub const PANE_TREE_EVENT: &str = "pane-tree";
pub const SIDE_SHEET_COMMAND_EVENT: &str = "side-sheet-command";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PaneTreeEvent {
    pub panes: Vec<PaneNode>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaneNode {
    pub id: u64,
    pub is_active: bool,
    pub tabs: Vec<TabNode>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TabNode {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub favicon_url: String,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub tab_index: usize,
    #[serde(default)]
    pub is_loading: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SideSheetCommandEvent {
    pub command: String,
    #[serde(default)]
    pub pane_id: String,
    #[serde(default)]
    pub tab_index: usize,
}

pub const SIDE_SHEET_DRAG_EVENT: &str = "side-sheet-drag";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SplitDirection {
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        tabs: Vec<TabNode>,
    },
}

#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(bevy_ecs::message::Message)
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum SideSheetDragCommand {
    MoveTab {
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
