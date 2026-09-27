use std::collections::BTreeSet;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use vmux_core::host::{UiState, UiStatePlugin, UiStateWrite};
use vmux_core::profile::vault::{GeneratedRecoveryKey, VaultRecovery};
use vmux_core::vault::{
    VaultAuthorization, VaultChooseCloudFolderRequest, VaultCompletion, VaultConnectCloudRequest,
    VaultConnectFolderRequest, VaultConnectGithubRequest, VaultConnectRequest,
    VaultConnectionProvider, VaultCreateCloudFolderRequest, VaultCreateRecoveryKeyRequest,
    VaultCreateRequest, VaultDestinationSelectRequest, VaultGenerateRecoveryKeyRequest,
    VaultNotice, VaultOperation, VaultOperationKind, VaultOperationState, VaultOwnerChoice,
    VaultOwnerKind, VaultOwnerSelectRequest, VaultPrivacyRequest, VaultProviderSelectRequest,
    VaultRecoveryConfirmationRequest, VaultRecoveryInputRequest, VaultRefreshRequest,
    VaultRepository, VaultRepositoryChoice, VaultRepositoryNameRequest,
    VaultRepositorySelectRequest, VaultSnapshot, VaultSyncRequest, VaultSyncStatus, VaultUiState,
    VaultUnlockRecoveryKeyRequest, VaultWorkflowConnectRequest, VaultWorkflowCreateRequest,
    VaultWorkflowState,
};

pub struct VaultPlugin;

impl Plugin for VaultPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::VaultPage::plugin());

        app.add_plugins((
            UiStatePlugin::<VaultUiState>::default(),
            UiEventPlugin::<(
                VaultCreateRequest,
                VaultConnectRequest,
                VaultSyncRequest,
                VaultConnectGithubRequest,
                VaultConnectFolderRequest,
                VaultGenerateRecoveryKeyRequest,
                VaultCreateRecoveryKeyRequest,
                VaultUnlockRecoveryKeyRequest,
                VaultConnectCloudRequest,
                VaultCreateCloudFolderRequest,
                VaultChooseCloudFolderRequest,
                VaultRefreshRequest,
            )>::default(),
            UiEventPlugin::<(
                VaultProviderSelectRequest,
                VaultDestinationSelectRequest,
                VaultOwnerSelectRequest,
                VaultRepositoryNameRequest,
                VaultRepositorySelectRequest,
                VaultPrivacyRequest,
                VaultWorkflowCreateRequest,
                VaultWorkflowConnectRequest,
                VaultRecoveryConfirmationRequest,
                VaultRecoveryInputRequest,
            )>::default(),
        ))
        .add_observer(on_vault_create_request)
        .add_observer(on_vault_connect_request)
        .add_observer(on_vault_sync_request)
        .add_observer(on_vault_connect_github_request)
        .add_observer(on_vault_connect_folder_request)
        .add_observer(on_vault_generate_recovery_key_request)
        .add_observer(on_vault_create_recovery_key_request)
        .add_observer(on_vault_unlock_recovery_key_request)
        .add_observer(on_vault_connect_cloud_request)
        .add_observer(on_vault_create_cloud_folder_request)
        .add_observer(on_vault_choose_cloud_folder_request)
        .add_observer(on_vault_refresh_request)
        .add_observer(on_vault_provider_select_request)
        .add_observer(on_vault_destination_select_request)
        .add_observer(on_vault_owner_select_request)
        .add_observer(on_vault_repository_name_request)
        .add_observer(on_vault_repository_select_request)
        .add_observer(on_vault_privacy_request)
        .add_observer(on_vault_workflow_create_request)
        .add_observer(on_vault_workflow_connect_request)
        .add_observer(on_vault_recovery_confirmation_request)
        .add_observer(on_vault_recovery_input_request)
        .add_systems(Startup, spawn_vault_runtime)
        .add_systems(
            Update,
            (
                drain_vault_watch,
                start_vault_scan,
                drain_vault_scan,
                queue_vault_auto_sync,
                start_vault_operation,
                emit_vault_state,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                launch_vault_create,
                launch_vault_connect,
                launch_vault_sync,
                launch_vault_connect_github,
                launch_vault_connect_folder,
                launch_vault_generate_recovery_key,
                launch_vault_create_recovery_key,
                launch_vault_unlock_recovery_key,
                launch_vault_connect_cloud,
                launch_vault_create_cloud_folder,
                launch_vault_choose_cloud_folder,
            ),
        )
        .add_systems(Update, drain_vault_operations);

        if let Some(watch) = VaultWatch::new(app) {
            app.insert_non_send(watch);
        }
    }
}

const VAULT_AUTO_SYNC_DELAY: Duration = Duration::from_secs(2);
const VAULT_REMOTE_SYNC_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Component)]
struct VaultRegistry {
    dirty: bool,
    loaded: bool,
    load_repositories: bool,
    generation: u64,
    revision: u64,
    snapshot: VaultSnapshot,
}

impl Default for VaultRegistry {
    fn default() -> Self {
        Self {
            dirty: true,
            loaded: false,
            load_repositories: false,
            generation: 1,
            revision: 0,
            snapshot: VaultSnapshot::default(),
        }
    }
}

#[derive(Component)]
struct VaultScanTask {
    generation: u64,
    task: Task<VaultSnapshot>,
}

#[derive(Component, Default)]
struct VaultOperationSequence(u64);

impl VaultOperationSequence {
    fn next(&mut self) -> u64 {
        let sequence = self.0;
        self.0 = self.0.wrapping_add(1);
        sequence
    }
}

#[derive(Component, Default)]
#[require(UiState<VaultUiState>, VaultWorkflow)]
struct VaultSubscriber {
    snapshot_revision: u64,
    revision: u64,
    emitted_revision: u64,
    state: VaultUiState,
}

#[derive(Component, Default)]
struct VaultWorkflow {
    state: VaultWorkflowState,
}

impl VaultWorkflow {
    fn for_url(url: &str) -> Self {
        let provider = url
            .split_once("?provider=")
            .and_then(|(_, query)| query.split(['&', '#']).next())
            .and_then(|provider| match provider {
                "github" => Some(VaultConnectionProvider::Github),
                "google_drive" | "cloud_folder" => Some(VaultConnectionProvider::GoogleDrive),
                "dropbox" => Some(VaultConnectionProvider::Dropbox),
                "onedrive" => Some(VaultConnectionProvider::OneDrive),
                _ => None,
            });
        Self {
            state: VaultWorkflowState {
                provider,
                ..Default::default()
            },
        }
    }

    fn projected(&self, state: &VaultUiState, snapshot_changed: bool) -> VaultWorkflowState {
        let vault = &state.vault;
        let mut projected = self.state.clone();
        if !vault.github_owner.is_empty()
            && (projected.selected_owner.is_empty()
                || !vault.github_owners.contains(&projected.selected_owner))
        {
            projected.selected_owner.clone_from(&vault.github_owner);
        }
        if snapshot_changed
            && vault.repositories_loaded
            && projected.repository_name == "vmux-vault"
        {
            projected.repository_name =
                Self::suggested_repository_name(&projected.selected_owner, &vault.repositories);
        }
        if vault.unlocked {
            projected.recovery_input.clear();
        }
        if state.generated_recovery_key.is_empty() {
            projected.recovery_confirmation.clear();
        }
        projected.recovery_confirmation_complete =
            Self::recovery_key_complete(&projected.recovery_confirmation);
        projected.recovery_confirmation_matches = Self::recovery_keys_match(
            &state.generated_recovery_key,
            &projected.recovery_confirmation,
        );
        projected.recovery_input_complete = Self::recovery_key_complete(&projected.recovery_input);
        projected.owners = vault
            .github_owners
            .iter()
            .map(|owner| VaultOwnerChoice {
                value: owner.clone(),
                kind: if owner == &vault.github_owner {
                    VaultOwnerKind::User
                } else {
                    VaultOwnerKind::Organization
                },
            })
            .collect();
        let owner_prefix = format!("{}/", projected.selected_owner);
        projected.repositories = vault
            .repositories
            .iter()
            .filter(|repository| repository.name.starts_with(&owner_prefix))
            .map(|repository| VaultRepositoryChoice {
                value: repository.url.clone(),
                name: repository.name.clone(),
                empty: repository.empty,
            })
            .collect();
        if !projected
            .repositories
            .iter()
            .any(|repository| repository.value == projected.selected_repository)
        {
            projected.selected_repository.clear();
        }
        projected.connected = vault.initialized && !vault.remote.is_empty();
        projected.pending = state
            .operation
            .as_ref()
            .filter(|operation| operation.is_pending())
            .map(|operation| operation.kind);
        projected.authenticated = projected.provider.is_some_and(|provider| {
            if provider.is_github() {
                !vault.github_owner.is_empty() && vault.repositories_loaded
            } else {
                !state.cloud_root.is_empty()
            }
        });
        projected.connecting = projected.pending.is_some_and(|kind| {
            kind == VaultOperationKind::ConnectGithub || kind == VaultOperationKind::ConnectCloud
        }) || projected.provider.is_some_and(|provider| {
            provider.is_github() && !vault.github_owner.is_empty() && !vault.repositories_loaded
        });
        let pending_changes = vault
            .dirty
            .saturating_add(vault.ahead)
            .saturating_add(vault.behind);
        projected.sync_status = if vault.sync_failed {
            VaultSyncStatus::Failed
        } else if pending_changes > 0 {
            VaultSyncStatus::Changes(pending_changes)
        } else {
            VaultSyncStatus::Clean
        };
        projected.notice = state.operation.as_ref().and_then(Self::notice);
        projected.github_device_code = state
            .operation
            .as_ref()
            .and_then(VaultOperation::authorization)
            .map(|authorization| authorization.code.clone())
            .unwrap_or_default();
        projected
    }

    fn notice(operation: &VaultOperation) -> Option<VaultNotice> {
        let completion = operation.completion()?;
        if completion.success
            && matches!(
                operation.kind,
                VaultOperationKind::GenerateRecoveryKey
                    | VaultOperationKind::CreateRecoveryKey
                    | VaultOperationKind::ConnectCloud
                    | VaultOperationKind::ConnectGithub
            )
        {
            return None;
        }
        let message_id =
            if completion.success {
                match operation.kind {
                    VaultOperationKind::Create => "vault-result-created",
                    VaultOperationKind::Connect => "vault-result-connected",
                    VaultOperationKind::Sync => "vault-result-synced",
                    VaultOperationKind::ConnectGithub => "vault-result-github-connected",
                    VaultOperationKind::ConnectFolder => "vault-result-folder-connected",
                    VaultOperationKind::GenerateRecoveryKey
                    | VaultOperationKind::CreateRecoveryKey => "vault-result-created",
                    VaultOperationKind::UnlockRecoveryKey => "vault-result-connected",
                    VaultOperationKind::ConnectCloud => "vault-result-connected",
                    VaultOperationKind::CreateCloudFolder
                    | VaultOperationKind::ChooseCloudFolder => "vault-result-folder-connected",
                }
            } else {
                match operation.kind {
                    VaultOperationKind::Sync => "vault-backup-failed",
                    VaultOperationKind::GenerateRecoveryKey
                    | VaultOperationKind::CreateRecoveryKey => "vault-recovery-key-create-failed",
                    VaultOperationKind::UnlockRecoveryKey => "vault-recovery-key-invalid",
                    _ => "",
                }
            };
        if !completion.success && message_id.is_empty() && completion.message.is_empty() {
            return None;
        }
        Some(VaultNotice {
            success: completion.success,
            message: completion.message.clone(),
            message_id: message_id.to_string(),
        })
    }

    fn suggested_repository_name(owner: &str, repositories: &[VaultRepository]) -> String {
        let prefix = format!("{owner}/");
        let names = repositories
            .iter()
            .filter_map(|repository| repository.name.strip_prefix(&prefix))
            .collect::<BTreeSet<_>>();
        if !names.contains("vmux-vault") {
            return "vmux-vault".to_string();
        }
        (2..)
            .map(|suffix| format!("vmux-vault-{suffix}"))
            .find(|name| !names.contains(name.as_str()))
            .unwrap()
    }

    fn normalized_recovery_key(value: &str) -> String {
        value
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|character| !character.is_ascii_whitespace() && *character != '-')
            .collect()
    }

    fn recovery_key_complete(value: &str) -> bool {
        Self::normalized_recovery_key(value).len() == 68
    }

    fn recovery_keys_match(expected: &str, actual: &str) -> bool {
        Self::recovery_key_complete(actual)
            && Self::normalized_recovery_key(expected) == Self::normalized_recovery_key(actual)
    }
}

impl VaultSubscriber {
    fn pending(operation_id: u64, kind: VaultOperationKind) -> Self {
        let mut subscriber = Self::default();
        subscriber.begin_operation(operation_id, kind);
        subscriber
    }

    fn begin_operation(&mut self, operation_id: u64, kind: VaultOperationKind) {
        match kind {
            VaultOperationKind::GenerateRecoveryKey => {
                self.state.generated_recovery_key.clear();
            }
            VaultOperationKind::ConnectCloud => {
                self.state.cloud_root.clear();
            }
            _ => {}
        }
        self.state.operation = Some(VaultOperation::pending(operation_id, kind));
        self.touch();
    }

    fn authorize(&mut self, operation_id: u64, authorization: VaultAuthorization) {
        let Some(operation) = self.state.operation.as_mut() else {
            return;
        };
        if operation.operation_id != operation_id {
            return;
        }
        operation.state = VaultOperationState::Authorizing(authorization);
        self.touch();
    }

    fn complete_operation(
        &mut self,
        operation_id: u64,
        kind: VaultOperationKind,
        completion: VaultCompletion,
    ) {
        if completion.success {
            match kind {
                VaultOperationKind::Sync => {
                    self.state.recovery_upload_pending = false;
                }
                VaultOperationKind::GenerateRecoveryKey => {
                    self.state
                        .generated_recovery_key
                        .clone_from(&completion.message);
                }
                VaultOperationKind::CreateRecoveryKey => {
                    self.state.generated_recovery_key.clear();
                    self.state.recovery_upload_pending = completion.pending_upload;
                }
                VaultOperationKind::ConnectCloud => {
                    self.state.cloud_root.clone_from(&completion.message);
                }
                _ => {}
            }
        }
        let Some(operation) = self.state.operation.as_mut() else {
            return;
        };
        if operation.operation_id != operation_id {
            return;
        }
        operation.state = VaultOperationState::Completed(completion);
        self.touch();
    }

    fn synchronize(&mut self, revision: u64, snapshot: &VaultSnapshot) -> bool {
        if self.snapshot_revision == revision {
            return false;
        }
        self.snapshot_revision = revision;
        self.state.vault = snapshot.clone();
        self.touch();
        true
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VaultOperationTarget {
    Webview(Entity),
    Automatic,
}

impl VaultOperationTarget {
    fn webview(self) -> Option<Entity> {
        match self {
            Self::Webview(entity) => Some(entity),
            Self::Automatic => None,
        }
    }
}

#[derive(Component)]
struct VaultOperationContext {
    operation_id: u64,
    target: VaultOperationTarget,
    kind: VaultOperationKind,
}

#[derive(Component, Default)]
struct PendingVaultOperation;

#[derive(Component, Default)]
struct ReadyVaultOperation;

#[derive(Component, Clone)]
struct VaultOperationRequest<R: Send + Sync + 'static>(R);

impl<R: Send + Sync + 'static> VaultOperationRequest<R> {
    fn new(request: R) -> Self {
        Self(request)
    }
}

#[derive(Component)]
struct VaultOperationTask {
    task: Task<Result<VaultOperationOutput, String>>,
    progress: Mutex<mpsc::Receiver<VaultAuthorization>>,
    canceled: Arc<AtomicBool>,
}

struct VaultOperationOutput {
    message: String,
    pending_upload: bool,
    generated_recovery_key: Option<GeneratedRecoveryKey>,
}

struct VaultWatch {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    debounce_tx: mpsc::Sender<()>,
    ready_rx: mpsc::Receiver<()>,
    remote_rx: mpsc::Receiver<()>,
}

impl VaultWatch {
    fn new(app: &App) -> Option<Self> {
        let vault_root = vmux_core::profile::vault::root_dir();
        let _ = std::fs::create_dir_all(&vault_root);
        let (watch_tx, watch_rx) = mpsc::channel();
        let watch_wake = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        let debounce_wake = watch_wake.clone();
        let remote_wake = watch_wake.clone();
        let mut watcher = match notify::recommended_watcher(move |result| {
            if watch_tx.send(result).is_ok()
                && let Some(wake) = &watch_wake
            {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                warn!("Vault watcher init failed: {error}");
                return None;
            }
        };
        if let Err(error) = watcher.watch(&vault_root, RecursiveMode::Recursive) {
            warn!("Vault watcher init failed: {error}");
            return None;
        }
        let (debounce_tx, debounce_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (remote_tx, remote_rx) = mpsc::channel();
        if let Err(error) = std::thread::Builder::new()
            .name("vmux-vault-auto-sync".to_string())
            .spawn(move || {
                loop {
                    if debounce_rx.recv().is_err() {
                        return;
                    }
                    loop {
                        match debounce_rx.recv_timeout(VAULT_AUTO_SYNC_DELAY) {
                            Ok(()) => {}
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                if ready_tx.send(()).is_ok()
                                    && let Some(wake) = &debounce_wake
                                {
                                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                                }
                                break;
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                }
            })
        {
            warn!("Vault auto-sync worker init failed: {error}");
        }
        if let Err(error) = std::thread::Builder::new()
            .name("vmux-vault-remote-sync".to_string())
            .spawn(move || {
                loop {
                    std::thread::sleep(VAULT_REMOTE_SYNC_INTERVAL);
                    if remote_tx.send(()).is_err() {
                        return;
                    }
                    if let Some(wake) = &remote_wake {
                        let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                    }
                }
            })
        {
            warn!("Vault remote-sync worker init failed: {error}");
        }
        Some(Self {
            _watcher: watcher,
            rx: watch_rx,
            debounce_tx,
            ready_rx,
            remote_rx,
        })
    }
}

#[derive(Component, Default)]
struct VaultAutoSync {
    requested: bool,
    initial_scan_complete: bool,
    remote_check: bool,
}

#[derive(Component)]
struct VaultRecoveryState {
    service: VaultRecovery,
    pending_key: Option<GeneratedRecoveryKey>,
}

impl Default for VaultRecoveryState {
    fn default() -> Self {
        Self {
            service: VaultRecovery::current(),
            pending_key: None,
        }
    }
}

impl VaultRecoveryState {
    fn service(&self) -> VaultRecovery {
        self.service.clone()
    }

    fn take_pending_key(&mut self) -> Option<GeneratedRecoveryKey> {
        self.pending_key.take()
    }

    fn retain(&mut self, key: GeneratedRecoveryKey) {
        self.pending_key = Some(key);
    }
}

type VaultOperationFuture =
    Pin<Box<dyn Future<Output = Result<VaultOperationOutput, String>> + Send>>;
type VaultProgress = Box<dyn Fn(VaultAuthorization) + Send>;
type VaultCancellation = Box<dyn Fn() -> bool + Send>;

impl VaultOperationOutput {
    fn message(message: String) -> Self {
        Self {
            message,
            pending_upload: false,
            generated_recovery_key: None,
        }
    }
}

fn create_vault(
    request: VaultCreateRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let visibility = if request.private {
            vmux_core::profile::vault::RepositoryVisibility::Private
        } else {
            vmux_core::profile::vault::RepositoryVisibility::Public
        };
        let message = vmux_core::profile::vault::create_remote(&request.repository, visibility)?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn connect_vault(
    request: VaultConnectRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = vmux_core::profile::vault::connect_remote(&request.repository)?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn sync_vault(
    _request: VaultSyncRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = vmux_core::profile::vault::sync()?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn connect_vault_github(
    _request: VaultConnectGithubRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    progress: VaultProgress,
    canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = vmux_core::profile::vault::connect_github_with_progress(
            |code| {
                progress(VaultAuthorization {
                    code,
                    url: "https://github.com/login/device".to_string(),
                });
            },
            canceled,
        )?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn connect_vault_folder(
    _request: VaultConnectFolderRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let initial_dir = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|home| home.join("Library/CloudStorage"))
            .filter(|path| path.is_dir())
            .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from));
        let mut dialog = rfd::AsyncFileDialog::new();
        if let Some(initial_dir) = initial_dir {
            dialog = dialog.set_directory(initial_dir);
        }
        let Some(folder) = dialog.pick_folder().await else {
            return Err(String::new());
        };
        let message = vmux_core::profile::vault::connect_folder(folder.path())?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn generate_vault_recovery_key(
    _request: VaultGenerateRecoveryKeyRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let key = GeneratedRecoveryKey::generate()?;
        Ok(VaultOperationOutput {
            message: key.display().to_string(),
            pending_upload: false,
            generated_recovery_key: Some(key),
        })
    })
}

fn create_vault_recovery_key(
    _request: VaultCreateRecoveryKeyRequest,
    recovery: VaultRecovery,
    generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let key = generated_recovery_key
            .ok_or_else(|| "No Recovery Key has been generated for this Vault".to_string())?;
        let result = recovery.create(key)?;
        Ok(VaultOperationOutput {
            message: String::new(),
            pending_upload: result.pending_upload,
            generated_recovery_key: None,
        })
    })
}

fn unlock_vault_recovery_key(
    request: VaultUnlockRecoveryKeyRequest,
    recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = recovery.unlock(&request.recovery_key)?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn connect_vault_cloud(
    request: VaultConnectCloudRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = connect_cloud_storage(&request.provider).await?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn create_vault_cloud_folder(
    request: VaultCreateCloudFolderRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let folder = Path::new(&request.root).join(&request.folder_name);
        let message = vmux_core::profile::vault::connect_folder(&folder)?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn choose_vault_cloud_folder(
    request: VaultChooseCloudFolderRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let mut dialog = rfd::AsyncFileDialog::new();
        let root = Path::new(&request.root);
        if root.is_dir() {
            dialog = dialog.set_directory(root);
        }
        let Some(folder) = dialog.pick_folder().await else {
            return Err(String::new());
        };
        let remote = if folder
            .path()
            .extension()
            .is_some_and(|extension| extension == "git")
        {
            folder.path().to_path_buf()
        } else {
            folder.path().join("vmux-vault.git")
        };
        if !remote.exists() {
            return Err("selected folder does not contain a Vault".to_string());
        }
        let message = vmux_core::profile::vault::connect_folder(folder.path())?;
        Ok(VaultOperationOutput::message(message))
    })
}

#[derive(SystemParam)]
struct VaultOperationQueue<'w, 's> {
    sequences: Query<'w, 's, &'static mut VaultOperationSequence, With<VaultRegistry>>,
    pending: Query<'w, 's, Entity, With<PendingVaultOperation>>,
    pending_github: Query<
        'w,
        's,
        Entity,
        (
            With<PendingVaultOperation>,
            With<VaultOperationRequest<VaultConnectGithubRequest>>,
        ),
    >,
    active_github: Query<
        'w,
        's,
        (Entity, Option<&'static VaultOperationTask>),
        (
            With<VaultOperationRequest<VaultConnectGithubRequest>>,
            Without<PendingVaultOperation>,
        ),
    >,
    subscribers: Query<'w, 's, &'static mut VaultSubscriber>,
    commands: Commands<'w, 's>,
}

impl VaultOperationQueue<'_, '_> {
    fn push<R: Send + Sync + 'static>(
        &mut self,
        target: Entity,
        request: R,
        kind: VaultOperationKind,
    ) {
        let Ok(mut sequence) = self.sequences.single_mut() else {
            return;
        };
        let operation_id = sequence.next();
        if let Ok(mut subscriber) = self.subscribers.get_mut(target) {
            subscriber.begin_operation(operation_id, kind);
        } else {
            self.commands
                .entity(target)
                .insert(VaultSubscriber::pending(operation_id, kind));
        }
        let mut connecting = false;
        for (entity, task) in &self.active_github {
            connecting = true;
            if let Some(task) = task {
                task.canceled.store(true, Ordering::Relaxed);
            } else {
                self.commands.entity(entity).despawn();
            }
        }
        if connecting {
            for entity in &self.pending {
                self.commands.entity(entity).despawn();
            }
        } else if kind == VaultOperationKind::ConnectGithub {
            for entity in &self.pending_github {
                self.commands.entity(entity).despawn();
            }
        }
        self.commands.spawn((
            PendingVaultOperation,
            VaultOperationContext {
                operation_id,
                target: VaultOperationTarget::Webview(target),
                kind,
            },
            VaultOperationRequest::new(request),
        ));
    }
}

fn on_vault_create_request(
    trigger: On<UiInput<VaultCreateRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::Create,
    );
}

fn on_vault_connect_request(
    trigger: On<UiInput<VaultConnectRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::Connect,
    );
}

fn on_vault_sync_request(trigger: On<UiInput<VaultSyncRequest>>, mut queue: VaultOperationQueue) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::Sync,
    );
}

fn on_vault_connect_github_request(
    trigger: On<UiInput<VaultConnectGithubRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::ConnectGithub,
    );
}

fn on_vault_connect_folder_request(
    trigger: On<UiInput<VaultConnectFolderRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::ConnectFolder,
    );
}

fn on_vault_generate_recovery_key_request(
    trigger: On<UiInput<VaultGenerateRecoveryKeyRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::GenerateRecoveryKey,
    );
}

fn on_vault_create_recovery_key_request(
    trigger: On<UiInput<VaultCreateRecoveryKeyRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::CreateRecoveryKey,
    );
}

fn on_vault_unlock_recovery_key_request(
    trigger: On<UiInput<VaultUnlockRecoveryKeyRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::UnlockRecoveryKey,
    );
}

fn on_vault_connect_cloud_request(
    trigger: On<UiInput<VaultConnectCloudRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::ConnectCloud,
    );
}

fn on_vault_create_cloud_folder_request(
    trigger: On<UiInput<VaultCreateCloudFolderRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::CreateCloudFolder,
    );
}

fn on_vault_choose_cloud_folder_request(
    trigger: On<UiInput<VaultChooseCloudFolderRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::ChooseCloudFolder,
    );
}

fn on_vault_refresh_request(
    trigger: On<UiInput<VaultRefreshRequest>>,
    mut registry: Query<&mut VaultRegistry>,
    pages: Query<&vmux_core::PageMetadata>,
    mut subscribers: Query<&mut VaultWorkflow, With<VaultSubscriber>>,
    mut commands: Commands,
) {
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    let webview = trigger.event().webview;
    let requested = pages
        .get(webview)
        .map(|page| VaultWorkflow::for_url(&page.url))
        .unwrap_or_default();
    if let Ok(mut workflow) = subscribers.get_mut(webview) {
        if workflow.state.provider.is_none() {
            workflow.state.provider = requested.state.provider;
        }
    } else {
        commands
            .entity(webview)
            .insert((VaultSubscriber::default(), requested));
    }
    state.dirty = true;
    state.loaded = false;
    state.load_repositories |= trigger.event().payload.load_repositories;
    state.generation = state.generation.wrapping_add(1);
    state.revision = state.revision.wrapping_add(1);
}

fn on_vault_provider_select_request(
    trigger: On<UiInput<VaultProviderSelectRequest>>,
    mut subscribers: Query<(&VaultSubscriber, &mut VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let provider = trigger.event().payload.provider;
    let Ok((subscriber, mut workflow)) = subscribers.get_mut(webview) else {
        return;
    };
    workflow.state.provider = Some(provider);
    workflow.state.destination = vmux_core::vault::VaultDestination::Create;
    workflow.state.selected_repository.clear();
    if provider.is_github() {
        if subscriber.state.vault.github_owner.is_empty() {
            commands.trigger(UiInput {
                webview,
                payload: VaultConnectGithubRequest,
            });
        }
        return;
    }
    workflow.state.repository_name = "vmux-vault".to_string();
    commands.trigger(UiInput {
        webview,
        payload: VaultConnectCloudRequest {
            provider: provider.name().to_string(),
        },
    });
}

fn on_vault_destination_select_request(
    trigger: On<UiInput<VaultDestinationSelectRequest>>,
    mut workflows: Query<&mut VaultWorkflow>,
) {
    let Ok(mut workflow) = workflows.get_mut(trigger.event().webview) else {
        return;
    };
    workflow.state.destination = trigger.event().payload.destination;
}

fn on_vault_owner_select_request(
    trigger: On<UiInput<VaultOwnerSelectRequest>>,
    mut subscribers: Query<(&VaultSubscriber, &mut VaultWorkflow)>,
) {
    let Ok((subscriber, mut workflow)) = subscribers.get_mut(trigger.event().webview) else {
        return;
    };
    let owner = &trigger.event().payload.owner;
    if !subscriber.state.vault.github_owners.contains(owner) {
        return;
    }
    workflow.state.selected_owner.clone_from(owner);
    workflow.state.repository_name =
        VaultWorkflow::suggested_repository_name(owner, &subscriber.state.vault.repositories);
    workflow.state.selected_repository.clear();
}

fn on_vault_repository_name_request(
    trigger: On<UiInput<VaultRepositoryNameRequest>>,
    mut workflows: Query<&mut VaultWorkflow>,
) {
    let Ok(mut workflow) = workflows.get_mut(trigger.event().webview) else {
        return;
    };
    workflow
        .state
        .repository_name
        .clone_from(&trigger.event().payload.name);
}

fn on_vault_repository_select_request(
    trigger: On<UiInput<VaultRepositorySelectRequest>>,
    mut workflows: Query<&mut VaultWorkflow>,
) {
    let Ok(mut workflow) = workflows.get_mut(trigger.event().webview) else {
        return;
    };
    workflow
        .state
        .selected_repository
        .clone_from(&trigger.event().payload.repository);
}

fn on_vault_privacy_request(
    trigger: On<UiInput<VaultPrivacyRequest>>,
    mut workflows: Query<&mut VaultWorkflow>,
) {
    let Ok(mut workflow) = workflows.get_mut(trigger.event().webview) else {
        return;
    };
    workflow.state.private = trigger.event().payload.private;
}

fn on_vault_workflow_create_request(
    trigger: On<UiInput<VaultWorkflowCreateRequest>>,
    workflows: Query<(&VaultSubscriber, &VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((subscriber, workflow)) = workflows.get(webview) else {
        return;
    };
    if subscriber
        .state
        .operation
        .as_ref()
        .is_some_and(VaultOperation::is_pending)
    {
        return;
    }
    let name = workflow.state.repository_name.trim();
    if name.is_empty() {
        return;
    }
    match workflow.state.provider {
        Some(VaultConnectionProvider::Github) if !workflow.state.selected_owner.is_empty() => {
            commands.trigger(UiInput {
                webview,
                payload: VaultCreateRequest {
                    repository: format!("{}/{}", workflow.state.selected_owner, name),
                    private: workflow.state.private,
                },
            });
        }
        Some(_) if !subscriber.state.cloud_root.is_empty() => {
            commands.trigger(UiInput {
                webview,
                payload: VaultCreateCloudFolderRequest {
                    root: subscriber.state.cloud_root.clone(),
                    folder_name: name.to_string(),
                },
            });
        }
        _ => {}
    }
}

fn on_vault_workflow_connect_request(
    trigger: On<UiInput<VaultWorkflowConnectRequest>>,
    workflows: Query<(&VaultSubscriber, &VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((subscriber, workflow)) = workflows.get(webview) else {
        return;
    };
    if subscriber
        .state
        .operation
        .as_ref()
        .is_some_and(VaultOperation::is_pending)
    {
        return;
    }
    match workflow.state.provider {
        Some(VaultConnectionProvider::Github) if !workflow.state.selected_repository.is_empty() => {
            commands.trigger(UiInput {
                webview,
                payload: VaultConnectRequest {
                    repository: workflow.state.selected_repository.clone(),
                },
            });
        }
        Some(_) if !subscriber.state.cloud_root.is_empty() => {
            commands.trigger(UiInput {
                webview,
                payload: VaultChooseCloudFolderRequest {
                    root: subscriber.state.cloud_root.clone(),
                },
            });
        }
        _ => {}
    }
}

fn on_vault_recovery_confirmation_request(
    trigger: On<UiInput<VaultRecoveryConfirmationRequest>>,
    mut subscribers: Query<(&VaultSubscriber, &mut VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((subscriber, mut workflow)) = subscribers.get_mut(webview) else {
        return;
    };
    workflow
        .state
        .recovery_confirmation
        .clone_from(&trigger.event().payload.value);
    workflow.state.recovery_confirmation_complete =
        VaultWorkflow::recovery_key_complete(&workflow.state.recovery_confirmation);
    workflow.state.recovery_confirmation_matches = VaultWorkflow::recovery_keys_match(
        &subscriber.state.generated_recovery_key,
        &workflow.state.recovery_confirmation,
    );
    let pending = subscriber
        .state
        .operation
        .as_ref()
        .is_some_and(VaultOperation::is_pending);
    if !pending && workflow.state.recovery_confirmation_matches {
        commands.trigger(UiInput {
            webview,
            payload: VaultCreateRecoveryKeyRequest,
        });
    }
}

fn on_vault_recovery_input_request(
    trigger: On<UiInput<VaultRecoveryInputRequest>>,
    mut subscribers: Query<(&VaultSubscriber, &mut VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((subscriber, mut workflow)) = subscribers.get_mut(webview) else {
        return;
    };
    workflow
        .state
        .recovery_input
        .clone_from(&trigger.event().payload.value);
    workflow.state.recovery_input_complete =
        VaultWorkflow::recovery_key_complete(&workflow.state.recovery_input);
    let pending = subscriber
        .state
        .operation
        .as_ref()
        .is_some_and(VaultOperation::is_pending);
    if !pending && workflow.state.recovery_input_complete {
        commands.trigger(UiInput {
            webview,
            payload: VaultUnlockRecoveryKeyRequest {
                recovery_key: workflow.state.recovery_input.clone(),
            },
        });
    }
}

fn spawn_vault_runtime(mut commands: Commands) {
    commands.spawn((
        Name::new("Vault"),
        VaultRegistry::default(),
        VaultOperationSequence::default(),
        VaultAutoSync::default(),
        VaultRecoveryState::default(),
    ));

    #[cfg(ui)]
    commands.spawn((
        crate::ui::VaultPage::MANIFEST,
        vmux_core::host::page::NativelyHosted::page(
            crate::ui::VaultPage::URL,
            crate::ui::VaultPage::NATIVE.title,
        ),
    ));
}

fn start_vault_scan(
    mut registries: Query<&mut VaultRegistry>,
    scans: Query<(), With<VaultScanTask>>,
    tasks: Query<(), With<VaultOperationTask>>,
    ready: Query<(), With<ReadyVaultOperation>>,
    pending: Query<(), With<PendingVaultOperation>>,
    mut commands: Commands,
) {
    let Ok(mut state) = registries.single_mut() else {
        return;
    };
    if !state.dirty
        || !scans.is_empty()
        || !tasks.is_empty()
        || !ready.is_empty()
        || !pending.is_empty()
    {
        return;
    }
    let generation = state.generation;
    let load_repositories = state.load_repositories;
    let previous = state.snapshot.clone();
    state.dirty = false;
    state.load_repositories = false;
    let task = IoTaskPool::get().spawn(async move { scan_vault(load_repositories, previous) });
    commands.spawn(VaultScanTask { generation, task });
}

fn drain_vault_scan(
    mut scans: Query<(Entity, &mut VaultScanTask)>,
    mut registries: Query<(&mut VaultRegistry, &mut VaultAutoSync)>,
    mut commands: Commands,
) {
    let Ok((mut state, mut auto_sync)) = registries.single_mut() else {
        return;
    };
    for (entity, mut scan) in &mut scans {
        let Some(snapshot) = future::block_on(future::poll_once(&mut scan.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if scan.generation != state.generation {
            state.dirty = true;
            continue;
        }
        state.snapshot = snapshot;
        state.loaded = true;
        state.revision = state.revision.wrapping_add(1);
        if !state.snapshot.github_owner.is_empty()
            && (!state.snapshot.initialized || state.snapshot.remote.is_empty())
            && !state.snapshot.repositories_loaded
        {
            state.dirty = true;
            state.load_repositories = true;
            state.generation = state.generation.wrapping_add(1);
        }
        if !auto_sync.initial_scan_complete {
            auto_sync.requested = state.snapshot.initialized
                && state.snapshot.unlocked
                && !state.snapshot.remote.is_empty()
                && (state.snapshot.dirty > 0
                    || state.snapshot.ahead > 0
                    || state.snapshot.behind > 0);
            auto_sync.initial_scan_complete = true;
        }
    }
}

fn drain_vault_watch(
    watcher: Option<NonSendMut<VaultWatch>>,
    mut registry: Query<(&mut VaultRegistry, &mut VaultAutoSync)>,
) {
    let Some(watcher) = watcher else {
        return;
    };
    let Ok((mut state, mut auto_sync)) = registry.single_mut() else {
        return;
    };
    if watcher.remote_rx.try_iter().next().is_some() {
        auto_sync.requested = true;
        auto_sync.remote_check = true;
    }
    let mut changed = false;
    for result in watcher.rx.try_iter() {
        changed |= vault_event_requests_sync(&result);
    }
    if !changed {
        if watcher.ready_rx.try_iter().next().is_some() {
            auto_sync.requested = true;
        }
        return;
    }
    state.dirty = true;
    state.generation = state.generation.wrapping_add(1);
    auto_sync.requested = false;
    auto_sync.remote_check = false;
    let _ = watcher.ready_rx.try_iter().count();
    let _ = watcher.debounce_tx.send(());
}

fn queue_vault_auto_sync(
    mut registry: Query<(
        &VaultRegistry,
        &mut VaultAutoSync,
        &mut VaultOperationSequence,
    )>,
    scans: Query<(), With<VaultScanTask>>,
    tasks: Query<
        (),
        (
            With<VaultOperationTask>,
            With<VaultOperationRequest<VaultSyncRequest>>,
        ),
    >,
    ready: Query<
        (),
        (
            With<ReadyVaultOperation>,
            With<VaultOperationRequest<VaultSyncRequest>>,
        ),
    >,
    pending: Query<
        (),
        (
            With<PendingVaultOperation>,
            With<VaultOperationRequest<VaultSyncRequest>>,
        ),
    >,
    mut commands: Commands,
) {
    let Ok((state, mut auto_sync, mut sequence)) = registry.single_mut() else {
        return;
    };
    if !auto_sync.requested || state.dirty || !state.loaded || !scans.is_empty() {
        return;
    }
    let vault = &state.snapshot;
    let sync_needed =
        auto_sync.remote_check || vault.dirty > 0 || vault.ahead > 0 || vault.behind > 0;
    if !vault.initialized || !vault.unlocked || vault.remote.is_empty() || !sync_needed {
        auto_sync.requested = false;
        auto_sync.remote_check = false;
        return;
    }
    if !tasks.is_empty() || !ready.is_empty() || !pending.is_empty() {
        auto_sync.requested = false;
        auto_sync.remote_check = false;
        return;
    }
    commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id: sequence.next(),
            target: VaultOperationTarget::Automatic,
            kind: VaultOperationKind::Sync,
        },
        VaultOperationRequest::new(VaultSyncRequest),
    ));
    auto_sync.requested = false;
    auto_sync.remote_check = false;
}

fn vault_event_requests_sync(result: &notify::Result<notify::Event>) -> bool {
    result.as_ref().is_ok_and(|event| {
        !matches!(event.kind, notify::EventKind::Access(_))
            && event
                .paths
                .iter()
                .any(|path| vmux_core::profile::vault::is_managed_local_path(path))
    })
}

fn start_vault_operation(
    pending: Query<(Entity, &VaultOperationContext), With<PendingVaultOperation>>,
    tasks: Query<(), With<VaultOperationTask>>,
    ready: Query<(), With<ReadyVaultOperation>>,
    scans: Query<(), With<VaultScanTask>>,
    mut commands: Commands,
) {
    if !tasks.is_empty() || !ready.is_empty() || !scans.is_empty() {
        return;
    }
    let mut next = None;
    for (entity, operation) in &pending {
        match next {
            Some((_, operation_id)) if operation_id <= operation.operation_id => {}
            _ => next = Some((entity, operation.operation_id)),
        }
    }
    let Some((entity, _)) = next else {
        return;
    };
    commands
        .entity(entity)
        .remove::<PendingVaultOperation>()
        .insert(ReadyVaultOperation);
}

type VaultOperationExecutor<R> = fn(
    R,
    VaultRecovery,
    Option<GeneratedRecoveryKey>,
    VaultProgress,
    VaultCancellation,
) -> VaultOperationFuture;

#[derive(SystemParam)]
struct VaultOperationLauncher<'w, 's> {
    recoveries: Query<'w, 's, &'static mut VaultRecoveryState, With<VaultRegistry>>,
    proxy: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
    commands: Commands<'w, 's>,
}

impl VaultOperationLauncher<'_, '_> {
    fn launch<R: Clone + Send + Sync + 'static>(
        &mut self,
        operations: &Query<(Entity, &VaultOperationRequest<R>), Added<ReadyVaultOperation>>,
        execute: VaultOperationExecutor<R>,
        take_pending_key: bool,
    ) {
        let Ok(mut recovery) = self.recoveries.single_mut() else {
            return;
        };
        for (entity, operation) in operations {
            let request = operation.0.clone();
            let service = recovery.service();
            let generated_recovery_key = if take_pending_key {
                recovery.take_pending_key()
            } else {
                None
            };
            let completion_wake = self.proxy.as_deref().map(|proxy| (**proxy).clone());
            let progress_wake = completion_wake.clone();
            let (progress_sender, progress_receiver) = mpsc::channel();
            let canceled = Arc::new(AtomicBool::new(false));
            let task_canceled = canceled.clone();
            let progress = Box::new(move |authorization| {
                if progress_sender.send(authorization).is_ok()
                    && let Some(wake) = &progress_wake
                {
                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                }
            });
            let cancellation = Box::new(move || task_canceled.load(Ordering::Relaxed));
            let operation = execute(
                request,
                service,
                generated_recovery_key,
                progress,
                cancellation,
            );
            let task = IoTaskPool::get().spawn(async move {
                let result = operation.await;
                if let Some(wake) = completion_wake {
                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                }
                result
            });
            self.commands
                .entity(entity)
                .remove::<ReadyVaultOperation>()
                .insert(VaultOperationTask {
                    task,
                    progress: Mutex::new(progress_receiver),
                    canceled,
                });
        }
    }
}

fn launch_vault_create(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultCreateRequest>),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, create_vault, false);
}

fn launch_vault_connect(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultConnectRequest>),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, connect_vault, false);
}

fn launch_vault_sync(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultSyncRequest>),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, sync_vault, false);
}

fn launch_vault_connect_github(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultConnectGithubRequest>),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, connect_vault_github, false);
}

fn launch_vault_connect_folder(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultConnectFolderRequest>),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, connect_vault_folder, false);
}

fn launch_vault_generate_recovery_key(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultGenerateRecoveryKeyRequest>,
        ),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, generate_vault_recovery_key, false);
}

fn launch_vault_create_recovery_key(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultCreateRecoveryKeyRequest>,
        ),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, create_vault_recovery_key, true);
}

fn launch_vault_unlock_recovery_key(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultUnlockRecoveryKeyRequest>,
        ),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, unlock_vault_recovery_key, false);
}

fn launch_vault_connect_cloud(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultConnectCloudRequest>),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, connect_vault_cloud, false);
}

fn launch_vault_create_cloud_folder(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultCreateCloudFolderRequest>,
        ),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, create_vault_cloud_folder, false);
}

fn launch_vault_choose_cloud_folder(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultChooseCloudFolderRequest>,
        ),
        Added<ReadyVaultOperation>,
    >,
    mut launcher: VaultOperationLauncher,
) {
    launcher.launch(&operations, choose_vault_cloud_folder, false);
}

fn drain_vault_operations(
    mut operations: Query<(Entity, &VaultOperationContext, &mut VaultOperationTask)>,
    mut registry: Query<(&mut VaultRegistry, &mut VaultRecoveryState)>,
    mut subscribers: Query<&mut VaultSubscriber>,
    mut stack_requests: MessageWriter<vmux_layout::stack::OpenRequest>,
    mut commands: Commands,
) {
    let Ok((mut state, mut recovery)) = registry.single_mut() else {
        return;
    };
    for (entity, context, mut task) in &mut operations {
        let target = context.target.webview();
        while let Ok(progress) = task.progress.get_mut().try_recv() {
            if let Some(target) = target {
                stack_requests.write(vmux_layout::stack::OpenRequest {
                    url: Some(progress.url.clone()),
                });
                if let Ok(mut subscriber) = subscribers.get_mut(target) {
                    subscriber.authorize(context.operation_id, progress);
                }
            }
        }
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if task.canceled.load(Ordering::Relaxed) {
            continue;
        }
        let (success, message, pending_upload, generated_recovery_key) = match result {
            Ok(output) => (
                true,
                output.message,
                output.pending_upload,
                output.generated_recovery_key,
            ),
            Err(message) => (false, message, false, None),
        };
        if let Some(key) = generated_recovery_key {
            recovery.retain(key);
        }
        let completion = VaultCompletion {
            success,
            message,
            pending_upload,
        };
        if context.kind == VaultOperationKind::Sync {
            state.snapshot.sync_failed = !success;
            state.revision = state.revision.wrapping_add(1);
            if context.target == VaultOperationTarget::Automatic && !success {
                continue;
            }
        }
        if let Some(target) = target
            && let Ok(mut subscriber) = subscribers.get_mut(target)
        {
            subscriber.complete_operation(context.operation_id, context.kind, completion);
        }
        state.dirty = true;
        state.loaded = false;
        state.load_repositories |= context.kind == VaultOperationKind::ConnectGithub;
        state.generation = state.generation.wrapping_add(1);
    }
}

fn emit_vault_state(
    registry: Query<&VaultRegistry>,
    mut subscribers: Query<(Entity, &mut VaultSubscriber, &mut VaultWorkflow)>,
    mut commands: Commands,
) {
    let Ok(state) = registry.single() else {
        return;
    };
    for (entity, mut subscriber, mut workflow) in &mut subscribers {
        let snapshot_changed = subscriber.synchronize(state.revision, &state.snapshot);
        let projected = workflow.projected(&subscriber.state, snapshot_changed);
        if workflow.state != projected {
            workflow.state = projected;
        }
        if subscriber.state.workflow != workflow.state {
            subscriber.state.workflow.clone_from(&workflow.state);
            subscriber.touch();
        }
        if subscriber.emitted_revision == subscriber.revision {
            continue;
        }
        commands.trigger(UiStateWrite::<VaultUiState>::from_event(
            entity,
            &subscriber.state,
        ));
        subscriber.emitted_revision = subscriber.revision;
    }
}

fn scan_vault(load_repositories: bool, previous: VaultSnapshot) -> VaultSnapshot {
    let status = if load_repositories {
        vmux_core::profile::vault::status_with_repositories()
    } else {
        vmux_core::profile::vault::status()
    };
    let mut snapshot = VaultSnapshot {
        root: status.root.to_string_lossy().into_owned(),
        initialized: status.initialized,
        encrypted: status.encrypted,
        unlocked: status.unlocked,
        vault_id: status.vault_id,
        recovery_enabled: status.recovery_enabled,
        remote: status.remote,
        branch: status.branch,
        dirty: status.dirty,
        ahead: status.ahead,
        behind: status.behind,
        sync_failed: previous.sync_failed,
        github_owner: status.github_owner,
        github_owners: status.github_owners,
        repositories: status
            .repositories
            .into_iter()
            .map(|repository| VaultRepository {
                name: repository.name,
                url: repository.url,
                private: repository.private,
                empty: repository.empty,
            })
            .collect(),
        repositories_loaded: load_repositories,
        error: status.error,
    };
    if !load_repositories && (!snapshot.initialized || snapshot.remote.is_empty()) {
        snapshot.github_owner = previous.github_owner;
        snapshot.github_owners = previous.github_owners;
        snapshot.repositories = previous.repositories;
        snapshot.repositories_loaded = previous.repositories_loaded;
        snapshot.error = previous.error;
    }
    snapshot
}

async fn connect_cloud_storage(provider: &str) -> Result<String, String> {
    let roots = cloud_storage_roots(provider);
    match roots.as_slice() {
        [] => Err(format!("{provider} is not connected on this device")),
        [root] => Ok(root.to_string_lossy().into_owned()),
        roots => {
            let initial = roots
                .first()
                .and_then(|root| root.parent())
                .unwrap_or_else(|| roots[0].as_path());
            let Some(folder) = rfd::AsyncFileDialog::new()
                .set_directory(initial)
                .pick_folder()
                .await
            else {
                return Err(String::new());
            };
            let selected = folder
                .path()
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if roots.iter().any(|root| {
                root.canonicalize()
                    .is_ok_and(|root| selected == root || selected.starts_with(root))
            }) {
                Ok(selected.to_string_lossy().into_owned())
            } else {
                Err(format!("selected folder is not in {provider}"))
            }
        }
    }
}

fn cloud_storage_roots(provider: &str) -> Vec<std::path::PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return Vec::new();
    };
    let prefixes: &[&str] = match provider {
        "Google Drive" => &["GoogleDrive"],
        "Dropbox" => &["Dropbox"],
        "OneDrive" => &["OneDrive"],
        _ => return Vec::new(),
    };
    let mut roots = std::fs::read_dir(home.join("Library/CloudStorage"))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            prefixes.iter().any(|prefix| name.starts_with(prefix))
        })
        .map(|entry| {
            let root = entry.path();
            if provider == "Google Drive" && root.join("My Drive").is_dir() {
                root.join("My Drive")
            } else {
                root
            }
        })
        .collect::<Vec<_>>();
    for fallback in prefixes.iter().map(|prefix| home.join(prefix)) {
        if fallback.is_dir() {
            roots.push(fallback);
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    struct VaultAutoSyncScenario;

    impl VaultAutoSyncScenario {
        fn pending_targets(vault: VaultSnapshot, remote_check: bool) -> Vec<VaultOperationTarget> {
            let mut app = App::new();
            app.add_systems(Update, queue_vault_auto_sync);
            let registry = app
                .world_mut()
                .spawn((
                    VaultRegistry::default(),
                    VaultAutoSync::default(),
                    VaultOperationSequence::default(),
                ))
                .id();
            {
                let mut state = app.world_mut().get_mut::<VaultRegistry>(registry).unwrap();
                state.dirty = false;
                state.loaded = true;
                state.snapshot = vault;
            }
            {
                let mut auto_sync = app.world_mut().get_mut::<VaultAutoSync>(registry).unwrap();
                auto_sync.requested = true;
                auto_sync.remote_check = remote_check;
            }

            app.update();

            let world = app.world_mut();
            let mut query =
                world.query_filtered::<&VaultOperationContext, With<PendingVaultOperation>>();
            query
                .iter(world)
                .map(|operation| operation.target)
                .collect()
        }
    }

    #[test]
    fn recovery_key_is_consumed_once() {
        let mut recovery = VaultRecoveryState::default();
        recovery.retain(GeneratedRecoveryKey::generate().unwrap());

        assert!(recovery.take_pending_key().is_some());
        assert!(recovery.take_pending_key().is_none());
    }

    #[test]
    fn vault_operation_state_preserves_recovery_workflow() {
        let mut subscriber = VaultSubscriber::pending(3, VaultOperationKind::GenerateRecoveryKey);
        subscriber.complete_operation(
            3,
            VaultOperationKind::GenerateRecoveryKey,
            VaultCompletion {
                success: true,
                message: "recovery-key".to_string(),
                pending_upload: false,
            },
        );
        assert_eq!(subscriber.state.generated_recovery_key, "recovery-key");

        subscriber.begin_operation(4, VaultOperationKind::CreateRecoveryKey);
        subscriber.complete_operation(
            4,
            VaultOperationKind::CreateRecoveryKey,
            VaultCompletion {
                success: true,
                message: String::new(),
                pending_upload: true,
            },
        );

        assert!(subscriber.state.generated_recovery_key.is_empty());
        assert!(subscriber.state.recovery_upload_pending);
    }

    #[test]
    fn vault_workflow_projects_choices_notice_and_recovery_validation() {
        let recovery_key = "a".repeat(68);
        let state = VaultUiState {
            vault: VaultSnapshot {
                github_owner: "jun".to_string(),
                github_owners: vec!["jun".to_string(), "vmux-ai".to_string()],
                repositories: vec![
                    VaultRepository {
                        name: "jun/vmux-vault".to_string(),
                        url: "first".to_string(),
                        private: true,
                        empty: false,
                    },
                    VaultRepository {
                        name: "jun/vmux-vault-2".to_string(),
                        url: "second".to_string(),
                        private: true,
                        empty: true,
                    },
                    VaultRepository {
                        name: "vmux-ai/shared".to_string(),
                        url: "other".to_string(),
                        private: false,
                        empty: false,
                    },
                ],
                repositories_loaded: true,
                dirty: 2,
                ..Default::default()
            },
            operation: Some(VaultOperation {
                operation_id: 8,
                kind: VaultOperationKind::Sync,
                state: VaultOperationState::Completed(VaultCompletion {
                    success: false,
                    message: "network".to_string(),
                    pending_upload: false,
                }),
            }),
            generated_recovery_key: recovery_key.clone(),
            workflow: VaultWorkflowState {
                recovery_confirmation: recovery_key,
                ..Default::default()
            },
            ..Default::default()
        };
        let workflow = VaultWorkflow {
            state: state.workflow.clone(),
        }
        .projected(&state, true);

        assert_eq!(workflow.selected_owner, "jun");
        assert_eq!(workflow.repository_name, "vmux-vault-3");
        assert_eq!(workflow.repositories.len(), 2);
        assert_eq!(workflow.owners[0].kind, VaultOwnerKind::User);
        assert_eq!(workflow.owners[1].kind, VaultOwnerKind::Organization);
        assert_eq!(workflow.sync_status, VaultSyncStatus::Changes(2));
        assert!(workflow.recovery_confirmation_complete);
        assert!(workflow.recovery_confirmation_matches);
        assert_eq!(
            workflow
                .notice
                .as_ref()
                .map(|notice| notice.message_id.as_str()),
            Some("vault-backup-failed")
        );
    }

    #[test]
    fn vault_workflow_reads_the_requested_provider_from_the_page_url() {
        assert_eq!(
            VaultWorkflow::for_url("vmux://vault/?provider=dropbox")
                .state
                .provider,
            Some(VaultConnectionProvider::Dropbox)
        );
        assert_eq!(
            VaultWorkflow::for_url("vmux://vault/?provider=cloud_folder")
                .state
                .provider,
            Some(VaultConnectionProvider::GoogleDrive)
        );
    }

    #[test]
    fn vault_backup_watcher_ignores_runtime_and_access_events() {
        let root = vmux_core::profile::vault::root_dir();
        let knowledge =
            notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
                .add_path(root.join("knowledge/note.md"));
        let runtime = notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(root.join("workspace/repo/file.rs"));
        let access = notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Any))
            .add_path(root.join("tools/tools.toml"));

        assert!(vault_event_requests_sync(&Ok(knowledge)));
        assert!(!vault_event_requests_sync(&Ok(runtime)));
        assert!(!vault_event_requests_sync(&Ok(access)));
    }

    #[test]
    fn automatic_backup_creates_only_needed_pending_operations() {
        let connected = VaultSnapshot {
            initialized: true,
            unlocked: true,
            remote: "https://example.com/vault.git".to_string(),
            dirty: 1,
            ..Default::default()
        };
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(connected.clone(), false),
            [VaultOperationTarget::Automatic]
        );
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(
                VaultSnapshot {
                    unlocked: false,
                    ..connected.clone()
                },
                false
            ),
            []
        );
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(
                VaultSnapshot {
                    dirty: 0,
                    ahead: 1,
                    ..connected.clone()
                },
                false
            ),
            [VaultOperationTarget::Automatic]
        );
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(
                VaultSnapshot {
                    dirty: 0,
                    ..connected.clone()
                },
                false
            ),
            []
        );
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(
                VaultSnapshot {
                    dirty: 0,
                    ..connected
                },
                true,
            ),
            [VaultOperationTarget::Automatic]
        );
    }
}
