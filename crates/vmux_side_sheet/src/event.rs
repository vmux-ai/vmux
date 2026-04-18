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
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SideSheetCommandEvent {
    pub command: String,
    #[serde(default)]
    pub pane_id: String,
    #[serde(default)]
    pub tab_index: usize,
}
