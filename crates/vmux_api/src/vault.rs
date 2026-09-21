#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum VaultProvider {
    CloudFolder,
    Github,
    Git,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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
