pub const PANE_TREE_EVENT: &str = "pane-tree";

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
}
