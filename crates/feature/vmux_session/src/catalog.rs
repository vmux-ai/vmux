#[vmux_api::contract(Default, Eq)]
pub struct CatalogSnapshot {
    pub stages: Vec<StageSummary>,
    pub sessions: Vec<SessionSummary>,
}

#[vmux_api::contract(Default, Eq)]
pub struct StageSummary {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub terminal: bool,
}

#[vmux_api::contract(Default, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cwd: String,
    pub stage: String,
    pub created_at: i64,
    pub last_activated_at: i64,
    pub stage_changed_at: i64,
    pub agent: String,
    pub runtime: String,
}
