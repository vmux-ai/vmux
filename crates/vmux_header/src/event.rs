pub const TABS_EVENT: &str = "tabs";
pub const RELOAD_EVENT: &str = "reload";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HeaderCommandEvent {
    pub header_command: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TabsHostEvent {
    pub tabs: Vec<TabRow>,
    #[serde(default)]
    pub can_go_back: bool,
    #[serde(default)]
    pub can_go_forward: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TabRow {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub favicon_url: String,
    pub is_active: bool,
}
