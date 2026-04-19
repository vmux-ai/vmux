pub const PALETTE_OPEN_EVENT: &str = "palette-open";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PaletteOpenEvent {
    pub url: String,
    pub tabs: Vec<PaletteTab>,
    pub commands: Vec<PaletteCommandEntry>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PaletteTab {
    pub title: String,
    pub url: String,
    pub pane_id: u64,
    pub tab_index: usize,
    pub is_active: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PaletteCommandEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PaletteActionEvent {
    pub action: String,
    pub value: String,
}
