pub const COMMAND_BAR_OPEN_EVENT: &str = "command-bar-open";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CommandBarOpenEvent {
    pub url: String,
    pub tabs: Vec<CommandBarTab>,
    pub commands: Vec<CommandBarCommandEntry>,
    pub new_tab: bool,
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

pub const PATH_COMPLETE_REQUEST: &str = "path-complete-request";
pub const PATH_COMPLETE_RESPONSE: &str = "path-complete-response";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PathCompleteRequest {
    pub query: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PathEntry {
    pub name: String,
    pub is_dir: bool,
    pub full_path: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PathCompleteResponse {
    pub completions: Vec<PathEntry>,
    /// Whether the exact queried path is an existing directory.
    #[serde(default)]
    pub query_is_dir: bool,
}
