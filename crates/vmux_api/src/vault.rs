#[vmux_api::contract(Copy, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VaultProvider {
    CloudFolder,
    Github,
    Git,
}

#[vmux_api::contract(Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatusSnapshot {
    pub root: String,
    pub connected: bool,
    pub encrypted: bool,
    pub unlocked: bool,
    pub recovery_key: bool,
    pub automatic_backup: bool,
    pub provider: Option<VaultProvider>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    pub branch: String,
    pub local_changes: u32,
    pub ahead: u32,
    pub behind: u32,
    pub sync_needed: bool,
}
