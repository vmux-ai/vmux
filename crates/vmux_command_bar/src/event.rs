pub const COMMAND_BAR_OPEN_EVENT: &str = "command-bar-open";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CommandBarOpenEvent {
    pub url: String,
    pub tabs: Vec<CommandBarTab>,
    pub commands: Vec<CommandBarCommandEntry>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandBarTab {
    pub title: String,
    pub url: String,
    pub pane_id: u64,
    pub tab_index: usize,
    pub is_active: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandBarCommandEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandBarActionEvent {
    pub action: String,
    pub value: String,
}
