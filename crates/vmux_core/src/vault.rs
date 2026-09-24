#[vmux_api::ui_state(Default, Eq, target = "vault")]
pub struct VaultSnapshot {
    pub root: String,
    pub initialized: bool,
    pub encrypted: bool,
    pub unlocked: bool,
    pub vault_id: String,
    pub recovery_enabled: bool,
    pub remote: String,
    pub branch: String,
    pub dirty: u32,
    pub ahead: u32,
    pub behind: u32,
    pub sync_failed: bool,
    pub github_owner: String,
    pub github_owners: Vec<String>,
    pub repositories: Vec<VaultRepository>,
    pub repositories_loaded: bool,
    pub error: String,
}

#[vmux_api::contract(Eq)]
pub struct VaultRepository {
    pub name: String,
    pub url: String,
    pub private: bool,
    pub empty: bool,
}

#[vmux_api::ui_event(Default, Eq, target = "vault")]
pub struct VaultRefreshRequest {
    #[serde(default)]
    pub load_repositories: bool,
}

#[vmux_api::contract(Copy, Eq)]
pub enum VaultAction {
    Create,
    Connect,
    Sync,
    ConnectGithub,
    ConnectFolder,
    GenerateRecoveryKey,
    CreateRecoveryKey,
    UnlockRecoveryKey,
    ConnectCloud,
    CreateCloudFolder,
    ChooseCloudFolder,
}

#[vmux_api::ui_event(Eq, target = "vault")]
pub struct VaultRequest {
    pub action: VaultAction,
    pub repository: String,
    pub private: bool,
    pub folder_name: String,
    pub recovery_key: String,
}

#[vmux_api::host_event(Eq, target = "vault")]
pub struct VaultResult {
    pub action: VaultAction,
    pub success: bool,
    pub message: String,
    pub pending_upload: bool,
}

#[vmux_api::host_event(Eq, target = "vault")]
pub struct VaultAuthProgress {
    pub code: String,
    pub url: String,
}
