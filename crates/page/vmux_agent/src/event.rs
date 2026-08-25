pub const AGENTS_CATALOG_EVENT: &str = "agents_catalog";

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
pub struct AgentsCatalog {
    pub agents: Vec<AgentEntry>,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentEntry {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub source: String,
    pub launch_url: String,
    pub uninstallable: bool,
    pub runtime: String,
    pub status: String,
    pub detail: String,
    pub pinned_version: String,
    pub installed_version: String,
    pub available_versions: Vec<String>,
}

impl AgentEntry {
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.name.to_lowercase().contains(&query)
            || self.id.to_lowercase().contains(&query)
            || self.description.to_lowercase().contains(&query)
            || self.runtime.to_lowercase().contains(&query)
            || self.source.to_lowercase().contains(&query)
    }
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
pub struct AgentsCatalogRequest {}

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
pub struct AgentsInstall {
    pub id: String,
    pub version: String,
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
pub struct AgentsUninstall {
    pub id: String,
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
pub struct AgentsOpen {
    pub url: String,
}
