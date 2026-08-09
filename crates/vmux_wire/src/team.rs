pub const TEAM_PAGE_URL: &str = "vmux://team/";
pub const TEAM_EVENT: &str = "team";

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
pub struct TeamEvent {
    pub members: Vec<TeamMemberRow>,
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
pub struct TeamMemberRow {
    pub id: String,
    pub name: String,
    pub initials: String,
    pub color: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub sid: String,
    pub is_user: bool,
    pub is_running: bool,
    #[serde(default)]
    pub is_done_unseen: bool,
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
pub struct TeamCommandEvent {
    pub command: String,
    #[serde(default)]
    pub member_id: Option<String>,
}

#[cfg(test)]
#[path = "team.test.rs"]
mod tests;
