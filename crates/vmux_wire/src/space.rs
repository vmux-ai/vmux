pub const SPACES_PAGE_URL: &str = "vmux://spaces/";
pub const SPACES_LIST_EVENT: &str = "spaces_list";

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
pub struct SpacesListEvent {
    pub spaces: Vec<SpaceRow>,
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
pub struct SpaceRow {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub is_active: bool,
    pub tab_count: u32,
    pub startup_dir: String,
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
pub struct SpaceCommandEvent {
    pub command: String,
    #[serde(default)]
    pub space_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[cfg(test)]
#[path = "space.test.rs"]
mod tests;
