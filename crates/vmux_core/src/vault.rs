pub const VAULT_SNAPSHOT_EVENT: &str = "vault-snapshot";
pub const VAULT_ACTION_RESULT_EVENT: &str = "vault-action-result";

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
pub struct VaultSnapshot {
    pub root: String,
    pub initialized: bool,
    pub encrypted: bool,
    pub vault_id: String,
    pub passkey_credentials: Vec<String>,
    pub passkey_salt: Vec<u8>,
    pub remote: String,
    pub branch: String,
    pub dirty: u32,
    pub ahead: u32,
    pub behind: u32,
    pub github_owner: String,
    pub repositories: Vec<VaultRepository>,
    pub repositories_loaded: bool,
    pub error: String,
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
pub struct VaultRepository {
    pub name: String,
    pub url: String,
    pub private: bool,
    pub empty: bool,
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
pub struct VaultRefreshRequest {
    #[serde(default)]
    pub load_repositories: bool,
}

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
pub enum VaultAction {
    Create,
    Connect,
    Sync,
    ConnectGithub,
    ConnectFolder,
    AddPasskey,
    UnlockPasskey,
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
pub struct VaultActionRequest {
    pub action: VaultAction,
    pub repository: String,
    pub private: bool,
    pub credential_id: String,
    pub prf_output: Vec<u8>,
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
pub struct VaultActionResult {
    pub action: VaultAction,
    pub success: bool,
    pub message: String,
}
