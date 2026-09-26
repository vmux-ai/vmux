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
    pub kind: VaultOperationKind,
    pub state: VaultOperationState,
}

impl VaultOperation {
    pub fn pending(operation_id: u64, kind: VaultOperationKind) -> Self {
        Self {
            operation_id,
            kind,
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

#[vmux_api::ui_state(Default, Eq, version = 4, url = "vmux://vault/")]
pub struct VaultUiState {
    pub vault: VaultSnapshot,
    pub operation: Option<VaultOperation>,
    pub generated_recovery_key: String,
    pub recovery_upload_pending: bool,
    pub cloud_root: String,
    pub workflow: VaultWorkflowState,
}

#[vmux_api::contract(Eq)]
pub struct VaultRepository {
    pub name: String,
    pub url: String,
    pub private: bool,
    pub empty: bool,
}

#[vmux_api::contract(Copy, Eq)]
pub enum VaultConnectionProvider {
    Github,
    GoogleDrive,
    Dropbox,
    OneDrive,
}

impl VaultConnectionProvider {
    pub const ALL: [Self; 4] = [
        Self::Github,
        Self::GoogleDrive,
        Self::Dropbox,
        Self::OneDrive,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Github => "GitHub",
            Self::GoogleDrive => "Google Drive",
            Self::Dropbox => "Dropbox",
            Self::OneDrive => "OneDrive",
        }
    }

    pub fn is_github(self) -> bool {
        self == Self::Github
    }
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum VaultDestination {
    #[default]
    Create,
    Existing,
}

#[vmux_api::contract(Copy, Eq)]
pub enum VaultOwnerKind {
    User,
    Organization,
}

#[vmux_api::contract(Eq)]
pub struct VaultOwnerChoice {
    pub value: String,
    pub kind: VaultOwnerKind,
}

#[vmux_api::contract(Eq)]
pub struct VaultRepositoryChoice {
    pub value: String,
    pub name: String,
    pub empty: bool,
}

#[vmux_api::contract(Eq, Default)]
pub enum VaultSyncStatus {
    Failed,
    Changes(u32),
    #[default]
    Clean,
}

#[vmux_api::contract(Eq)]
pub struct VaultNotice {
    pub success: bool,
    pub message: String,
    pub message_id: String,
}

#[vmux_api::contract(Eq)]
pub struct VaultWorkflowState {
    pub provider: Option<VaultConnectionProvider>,
    pub destination: VaultDestination,
    pub repository_name: String,
    pub selected_owner: String,
    pub selected_repository: String,
    pub private: bool,
    pub owners: Vec<VaultOwnerChoice>,
    pub repositories: Vec<VaultRepositoryChoice>,
    pub connected: bool,
    pub authenticated: bool,
    pub connecting: bool,
    pub pending: Option<VaultOperationKind>,
    pub sync_status: VaultSyncStatus,
    pub notice: Option<VaultNotice>,
    pub github_device_code: String,
    pub recovery_confirmation: String,
    pub recovery_confirmation_complete: bool,
    pub recovery_confirmation_matches: bool,
    pub recovery_input: String,
    pub recovery_input_complete: bool,
}

impl Default for VaultWorkflowState {
    fn default() -> Self {
        Self {
            provider: None,
            destination: VaultDestination::Create,
            repository_name: "vmux-vault".to_string(),
            selected_owner: String::new(),
            selected_repository: String::new(),
            private: true,
            owners: Vec::new(),
            repositories: Vec::new(),
            connected: false,
            authenticated: false,
            connecting: false,
            pending: None,
            sync_status: VaultSyncStatus::Clean,
            notice: None,
            github_device_code: String::new(),
            recovery_confirmation: String::new(),
            recovery_confirmation_complete: false,
            recovery_confirmation_matches: false,
            recovery_input: String::new(),
            recovery_input_complete: false,
        }
    }
}

#[vmux_api::ui_event(Default, Eq, url = "vmux://vault/")]
pub struct VaultRefreshRequest {
    #[serde(default)]
    pub load_repositories: bool,
}

#[vmux_api::contract(Copy, Eq)]
pub enum VaultOperationKind {
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

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultCreateRequest {
    pub repository: String,
    pub private: bool,
}

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultConnectRequest {
    pub repository: String,
}

#[vmux_api::ui_event(Default, Eq, url = "vmux://vault/")]
pub struct VaultSyncRequest;

#[vmux_api::ui_event(Default, Eq, url = "vmux://vault/")]
pub struct VaultConnectGithubRequest;

#[vmux_api::ui_event(Default, Eq, url = "vmux://vault/")]
pub struct VaultConnectFolderRequest;

#[vmux_api::ui_event(Default, Eq, url = "vmux://vault/")]
pub struct VaultGenerateRecoveryKeyRequest;

#[vmux_api::ui_event(Default, Eq, url = "vmux://vault/")]
pub struct VaultCreateRecoveryKeyRequest;

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultUnlockRecoveryKeyRequest {
    pub recovery_key: String,
}

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultConnectCloudRequest {
    pub provider: String,
}

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultCreateCloudFolderRequest {
    pub root: String,
    pub folder_name: String,
}

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultChooseCloudFolderRequest {
    pub root: String,
}

#[vmux_api::ui_event(Copy, Eq, url = "vmux://vault/")]
pub struct VaultProviderSelectRequest {
    pub provider: VaultConnectionProvider,
}

#[vmux_api::ui_event(Copy, Eq, url = "vmux://vault/")]
pub struct VaultDestinationSelectRequest {
    pub destination: VaultDestination,
}

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultOwnerSelectRequest {
    pub owner: String,
}

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultRepositoryNameRequest {
    pub name: String,
}

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultRepositorySelectRequest {
    pub repository: String,
}

#[vmux_api::ui_event(Copy, Eq, url = "vmux://vault/")]
pub struct VaultPrivacyRequest {
    pub private: bool,
}

#[vmux_api::ui_event(Default, Eq, url = "vmux://vault/")]
pub struct VaultWorkflowCreateRequest;

#[vmux_api::ui_event(Default, Eq, url = "vmux://vault/")]
pub struct VaultWorkflowConnectRequest;

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultRecoveryConfirmationRequest {
    pub value: String,
}

#[vmux_api::ui_event(Eq, url = "vmux://vault/")]
pub struct VaultRecoveryInputRequest {
    pub value: String,
}
