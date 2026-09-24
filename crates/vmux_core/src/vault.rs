#[vmux_api::contract(Default, Eq)]
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
pub struct VaultAuthorization {
    pub code: String,
    pub url: String,
}

#[vmux_api::contract(Eq)]
pub struct VaultCompletion {
    pub success: bool,
    pub message: String,
    pub pending_upload: bool,
}

#[vmux_api::contract(Eq)]
pub enum VaultOperationState {
    Pending,
    Authorizing(VaultAuthorization),
    Completed(VaultCompletion),
}

#[vmux_api::contract(Eq)]
pub struct VaultOperation {
    pub operation_id: u64,
    pub action: VaultAction,
    pub state: VaultOperationState,
}

impl VaultOperation {
    pub fn pending(operation_id: u64, action: VaultAction) -> Self {
        Self {
            operation_id,
            action,
            state: VaultOperationState::Pending,
        }
    }

    pub fn is_pending(&self) -> bool {
        !matches!(self.state, VaultOperationState::Completed(_))
    }

    pub fn authorization(&self) -> Option<&VaultAuthorization> {
        let VaultOperationState::Authorizing(authorization) = &self.state else {
            return None;
        };
        Some(authorization)
    }

    pub fn completion(&self) -> Option<&VaultCompletion> {
        let VaultOperationState::Completed(completion) = &self.state else {
            return None;
        };
        Some(completion)
    }
}

#[vmux_api::ui_state(Default, Eq, version = 2, target = "vault")]
pub struct VaultUiState {
    pub vault: VaultSnapshot,
    pub operation: Option<VaultOperation>,
    pub generated_recovery_key: String,
    pub recovery_upload_pending: bool,
    pub cloud_root: String,
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
