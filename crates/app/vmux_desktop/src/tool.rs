use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use vmux_core::host::{UiState, UiStatePlugin, UiStateWrite};
use vmux_core::profile::vault::{GeneratedRecoveryKey, VaultRecovery};
use vmux_core::tool::{
    ToolAdoptRequest, ToolApplyRequest, ToolCategory, ToolForgetRequest, ToolImportRequest,
    ToolInstallRequest, ToolItem, ToolLinkRequest, ToolOpenRequest, ToolOperationKey,
    ToolOperationKind, ToolOperationNotice, ToolProvider, ToolStatus, ToolUninstallRequest,
    ToolUnlinkRequest, ToolUpdateRequest, ToolsNavigateRequest, ToolsRefreshRequest, ToolsSnapshot,
    ToolsUiState,
};
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
use vmux_tool::{
    ExternalToolOperation, ToolOperationCompletion, ToolOperationRequest, ToolStore,
    ToolStoreOperation, ToolStoreTarget, ToolsManifest,
};

pub(crate) struct ToolUiPlugin;

impl Plugin for ToolUiPlugin {
    fn build(&self, app: &mut App) {
        let vault_root = vmux_core::profile::vault::root_dir();
        let _ = std::fs::create_dir_all(&vault_root);
        let (watch_tx, watch_rx) = mpsc::channel();
        let watch_wake = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        let debounce_wake = watch_wake.clone();
        let remote_wake = watch_wake.clone();
        match notify::recommended_watcher(move |result| {
            if watch_tx.send(result).is_ok()
                && let Some(wake) = &watch_wake
            {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        }) {
            Ok(mut watcher) => {
                if watcher.watch(&vault_root, RecursiveMode::Recursive).is_ok() {
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
                                                let _ = wake.send_event(
                                                    bevy::winit::WinitUserEvent::WakeUp,
                                                );
                                            }
                                            break;
                                        }
                                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                                    }
                                }
                            }
                        })
                    {
                        bevy::log::warn!("Vault auto-sync worker init failed: {error}");
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
                        bevy::log::warn!("Vault remote-sync worker init failed: {error}");
                    }
                    app.insert_non_send(VaultWatch {
                        _watcher: watcher,
                        rx: watch_rx,
                        debounce_tx,
                        ready_rx,
                        remote_rx,
                    });
                }
            }
            Err(error) => bevy::log::warn!("Vault watcher init failed: {error}"),
        }
        app.world_mut().spawn((
            vmux_layout::tool_page::ToolsPage::MANIFEST,
            vmux_core::host::page::NativelyHosted::subtree(
                vmux_layout::tool_page::ToolsPage::URL,
                vmux_layout::tool_page::ToolsPage::NATIVE.title,
            ),
        ));
        app.world_mut().spawn((
            vmux_layout::vault_page::VaultPage::MANIFEST,
            vmux_core::host::page::NativelyHosted::page(
                vmux_layout::vault_page::VaultPage::URL,
                vmux_layout::vault_page::VaultPage::NATIVE.title,
            ),
        ));
        app.world_mut().spawn((
            Name::new("Tool registry"),
            ToolRegistry::default(),
            ToolStore::current(),
            ToolsManifest::default(),
            OperationRequestSequence::default(),
            VaultAutoSync::default(),
            VaultRecoveryState::default(),
        ));
        app.add_plugins((
            vmux_app::extension::McpConnectionPlugin,
            UiStatePlugin::<ToolsUiState>::default(),
            UiStatePlugin::<VaultUiState>::default(),
        ))
        .add_plugins((
            UiEventPlugin::<(
                ToolsRefreshRequest,
                ToolInstallRequest,
                ToolUpdateRequest,
                ToolUninstallRequest,
                ToolForgetRequest,
                ToolAdoptRequest,
                ToolLinkRequest,
                ToolUnlinkRequest,
                ToolApplyRequest,
                ToolImportRequest,
                ToolOpenRequest,
                ToolsNavigateRequest,
            )>::default(),
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
        .add_observer(on_refresh_request)
        .add_observer(on_install_request)
        .add_observer(on_update_request)
        .add_observer(on_uninstall_request)
        .add_observer(on_forget_request)
        .add_observer(on_adopt_request)
        .add_observer(on_link_request)
        .add_observer(on_unlink_request)
        .add_observer(on_apply_request)
        .add_observer(on_import_request)
        .add_observer(on_navigate_request)
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
        .add_observer(on_open_request)
        .add_systems(
            Update,
            (
                drain_vault_watch,
                start_tools_scan,
                drain_tools_scan,
                queue_vault_auto_sync,
                start_tool_operation,
                drain_tool_operations,
                start_vault_operation,
                emit_tools_state,
                emit_vault_state,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                start_tool_install,
                start_tool_update,
                start_tool_uninstall,
                start_tool_forget,
                start_tool_adopt,
                start_tool_link,
                start_tool_unlink,
                start_tool_apply,
                start_tool_import,
            ),
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
        .add_systems(Update, drain_vault_operations)
        .add_systems(Update, drain_tool_store_operations.before(emit_tools_state));
    }
}

const VAULT_AUTO_SYNC_DELAY: Duration = Duration::from_secs(2);
const VAULT_REMOTE_SYNC_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Component)]
struct ToolRegistry {
    dirty: bool,
    full_scan: bool,
    refresh_catalogs: bool,
    load_vault_repositories: bool,
    generation: u64,
    revision: u64,
    snapshot: ToolsSnapshot,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            dirty: true,
            full_scan: true,
            refresh_catalogs: false,
            load_vault_repositories: false,
            generation: 1,
            revision: 0,
            snapshot: ToolsSnapshot::default(),
        }
    }
}

#[derive(Component, Default)]
#[require(UiState<ToolsUiState>)]
struct ToolSubscriber {
    snapshot_revision: u64,
    revision: u64,
    emitted_revision: u64,
    pending: BTreeMap<u64, ToolOperationKey>,
    state: ToolsUiState,
}

impl ToolSubscriber {
    fn pending(operation_id: u64, operation: ToolOperationKey) -> Self {
        let mut subscriber = Self::default();
        subscriber.begin(operation_id, operation);
        subscriber
    }

    fn begin(&mut self, operation_id: u64, operation: ToolOperationKey) {
        self.pending.insert(operation_id, operation);
        self.state.pending = self.pending.values().cloned().collect();
        self.state.notice = None;
        self.touch();
    }

    fn complete(
        &mut self,
        operation_id: u64,
        fallback: ToolOperationKey,
        success: bool,
        message: String,
    ) {
        let operation = self.pending.remove(&operation_id).unwrap_or(fallback);
        self.state.pending = self.pending.values().cloned().collect();
        self.state.notice = Some(ToolOperationNotice {
            operation,
            success,
            message,
        });
        self.touch();
    }

    fn synchronize(&mut self, revision: u64, snapshot: &ToolsSnapshot) {
        if self.snapshot_revision == revision {
            return;
        }
        self.snapshot_revision = revision;
        self.state.snapshot = snapshot.clone();
        self.touch();
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
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

#[derive(Component)]
struct ToolsScanTask {
    generation: u64,
    task: Task<ToolsScanOutput>,
}

struct ToolsScanOutput {
    snapshot: ToolsSnapshot,
    manifest: ToolsManifest,
}

#[derive(Component)]
struct ToolOperationTask {
    task: Task<Result<String, String>>,
}

#[derive(Component, Default)]
struct OperationRequestSequence(u64);

impl OperationRequestSequence {
    fn next(&mut self) -> u64 {
        let order = self.0;
        self.0 = self.0.wrapping_add(1);
        order
    }
}

#[derive(Component)]
struct ToolOperationContext {
    operation_id: u64,
    target: Entity,
    operation: ToolOperationKey,
}

#[derive(Component, Default)]
struct PendingToolOperation;

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

    fn request(&self) -> &R {
        &self.0
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

#[derive(Clone, Debug)]
struct InventoryItem {
    id: String,
    name: String,
    icon: Option<String>,
    version: Option<String>,
    detail: String,
    status: ToolStatus,
    removable: bool,
}

fn install_tool(request: ToolInstallRequest, store: &ToolStore) -> Result<String, String> {
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    set_manifest_entry(store, request.provider, &request.id, true)?;
    install_provider(store, request.provider, &request.id)?;
    Ok(format!("{} installed", request.id))
}

fn update_tool(request: ToolUpdateRequest, store: &ToolStore) -> Result<String, String> {
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    set_manifest_entry(store, request.provider, &request.id, true)?;
    update_provider(store, request.provider, &request.id)?;
    Ok(format!("{} updated", request.id))
}

fn uninstall_tool(request: ToolUninstallRequest, store: &ToolStore) -> Result<String, String> {
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    uninstall_provider(store, request.provider, &request.id)?;
    set_manifest_entry(store, request.provider, &request.id, false)?;
    Ok(format!("{} removed", request.id))
}

fn forget_tool(request: ToolForgetRequest, store: &ToolStore) -> Result<String, String> {
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    set_manifest_entry(store, request.provider, &request.id, false)?;
    Ok(format!("{} removed from tools.toml", request.id))
}

fn adopt_tool(request: ToolAdoptRequest, store: &ToolStore) -> Result<String, String> {
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    if matches!(request.provider, ToolProvider::Dotfiles | ToolProvider::Mcp) {
        return Err(format!(
            "{} adopt request reached the desktop fallback",
            request.provider.id()
        ));
    }
    set_manifest_entry(store, request.provider, &request.id, true)?;
    Ok(format!("{} is now managed", request.id))
}

fn link_tool(request: ToolLinkRequest, store: &ToolStore) -> Result<String, String> {
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    if request.provider != ToolProvider::Dotfiles {
        return Err("link is only valid for dotfiles".to_string());
    }
    set_manifest_entry(store, request.provider, &request.id, true)?;
    let linked =
        vmux_tool::apply_dotfile_package_in(&store.dotfiles_dir(), store.home(), &request.id)?;
    Ok(format!("linked {linked} file(s)"))
}

fn unlink_tool(request: ToolUnlinkRequest, store: &ToolStore) -> Result<String, String> {
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    if request.provider != ToolProvider::Dotfiles {
        return Err("unlink is only valid for dotfiles".to_string());
    }
    let _ = store.load()?;
    let removed = vmux_tool::disable_and_unlink_dotfile_package_in(
        &store.manifest_path(),
        &store.dotfiles_dir(),
        store.home(),
        &request.id,
    )?;
    Ok(format!("unlinked {removed} file(s)"))
}

fn apply_tools(_request: ToolApplyRequest, store: &ToolStore) -> Result<String, String> {
    apply_manifest(store)
}

fn import_tools(request: ToolImportRequest, store: &ToolStore) -> Result<String, String> {
    import_provider(store, request.provider, request.value.trim())
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

fn on_open_request(
    trigger: On<UiInput<ToolOpenRequest>>,
    stores: Query<&ToolStore, With<ToolRegistry>>,
    mut requests: MessageWriter<vmux_layout::stack::OpenRequest>,
) {
    let path = Path::new(trigger.event().payload.path.trim());
    let Ok(store) = stores.single() else {
        return;
    };
    if path == store.brewfile_path() && !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, "");
    }
    let Ok(url) = url::Url::from_file_path(path) else {
        return;
    };
    requests.write(vmux_layout::stack::OpenRequest {
        url: Some(url.to_string()),
    });
}

fn on_navigate_request(
    trigger: On<UiInput<ToolsNavigateRequest>>,
    parents: Query<&ChildOf>,
    stacks: Query<(), With<vmux_layout::stack::Stack>>,
    mut requests: MessageWriter<vmux_core::PageOpenRequest>,
) {
    let Some(url) = trigger.event().payload.canonical_url() else {
        return;
    };
    let mut current = trigger.event().webview;
    loop {
        if stacks.contains(current) {
            requests.write(vmux_core::PageOpenRequest {
                target: vmux_core::PageOpenTarget::Stack(current),
                url: url.to_string(),
                request_id: None,
            });
            return;
        }
        let Ok(parent) = parents.get(current) else {
            return;
        };
        current = parent.parent();
    }
}

fn on_refresh_request(
    trigger: On<UiInput<ToolsRefreshRequest>>,
    mut registry: Query<&mut ToolRegistry>,
    subscribers: Query<(), With<ToolSubscriber>>,
    mut commands: Commands,
) {
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    let request = &trigger.event().payload;
    if !subscribers.contains(trigger.event().webview) {
        commands
            .entity(trigger.event().webview)
            .insert(ToolSubscriber::default());
    }
    if request.refresh || !state.snapshot.loaded {
        state.dirty = true;
        state.full_scan = true;
        state.refresh_catalogs |= request.refresh;
        state.generation = state.generation.wrapping_add(1);
        if request.refresh && state.snapshot.loaded {
            state.snapshot.loaded = false;
            state.revision = state.revision.wrapping_add(1);
        }
    }
}

fn queue_tool_operation<R: Clone + Send + Sync + 'static>(
    target: Entity,
    request: R,
    operation: ToolOperationKey,
    mut registries: Query<&mut OperationRequestSequence, With<ToolRegistry>>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut commands: Commands,
) {
    let Ok(mut sequence) = registries.single_mut() else {
        return;
    };
    let operation_id = sequence.next();
    if let Ok(mut subscriber) = subscribers.get_mut(target) {
        subscriber.begin(operation_id, operation.clone());
    } else {
        commands
            .entity(target)
            .insert(ToolSubscriber::pending(operation_id, operation.clone()));
    }
    commands.spawn((
        PendingToolOperation,
        ToolOperationContext {
            operation_id,
            target,
            operation,
        },
        ToolOperationRequest::new(request),
    ));
}

macro_rules! tool_operation_observer {
    ($name:ident, $request:ty, $operation:expr) => {
        fn $name(
            trigger: On<UiInput<$request>>,
            registries: Query<&mut OperationRequestSequence, With<ToolRegistry>>,
            subscribers: Query<&mut ToolSubscriber>,
            commands: Commands,
        ) {
            let request = trigger.event().payload.clone();
            queue_tool_operation(
                trigger.event().webview,
                request.clone(),
                ($operation)(request),
                registries,
                subscribers,
                commands,
            );
        }
    };
}

tool_operation_observer!(
    on_install_request,
    ToolInstallRequest,
    |request: ToolInstallRequest| {
        ToolOperationKey::new(request.provider, ToolOperationKind::Install, request.id)
    }
);
tool_operation_observer!(
    on_update_request,
    ToolUpdateRequest,
    |request: ToolUpdateRequest| {
        ToolOperationKey::new(request.provider, ToolOperationKind::Update, request.id)
    }
);
tool_operation_observer!(
    on_uninstall_request,
    ToolUninstallRequest,
    |request: ToolUninstallRequest| {
        ToolOperationKey::new(request.provider, ToolOperationKind::Uninstall, request.id)
    }
);
tool_operation_observer!(
    on_forget_request,
    ToolForgetRequest,
    |request: ToolForgetRequest| {
        ToolOperationKey::new(request.provider, ToolOperationKind::Forget, request.id)
    }
);
tool_operation_observer!(
    on_adopt_request,
    ToolAdoptRequest,
    |request: ToolAdoptRequest| {
        ToolOperationKey::new(request.provider, ToolOperationKind::Adopt, request.id)
    }
);
tool_operation_observer!(
    on_link_request,
    ToolLinkRequest,
    |request: ToolLinkRequest| {
        ToolOperationKey::new(request.provider, ToolOperationKind::Link, request.id)
    }
);
tool_operation_observer!(
    on_unlink_request,
    ToolUnlinkRequest,
    |request: ToolUnlinkRequest| {
        ToolOperationKey::new(request.provider, ToolOperationKind::Unlink, request.id)
    }
);
tool_operation_observer!(
    on_apply_request,
    ToolApplyRequest,
    |_request: ToolApplyRequest| {
        ToolOperationKey::new(ToolProvider::Dotfiles, ToolOperationKind::Apply, "")
    }
);
tool_operation_observer!(
    on_import_request,
    ToolImportRequest,
    |request: ToolImportRequest| {
        ToolOperationKey::new(request.provider, ToolOperationKind::Import, "")
    }
);

fn queue_vault_operation<R: Clone + Send + Sync + 'static>(
    target: Entity,
    request: R,
    kind: VaultOperationKind,
    mut registries: Query<&mut OperationRequestSequence, With<ToolRegistry>>,
    pending: Query<Entity, With<PendingVaultOperation>>,
    pending_github: Query<
        Entity,
        (
            With<PendingVaultOperation>,
            With<VaultOperationRequest<VaultConnectGithubRequest>>,
        ),
    >,
    active_github: Query<
        (Entity, Option<&VaultOperationTask>),
        (
            With<VaultOperationRequest<VaultConnectGithubRequest>>,
            Without<PendingVaultOperation>,
        ),
    >,
    mut subscribers: Query<&mut VaultSubscriber>,
    mut commands: Commands,
) {
    let Ok(mut sequence) = registries.single_mut() else {
        return;
    };
    let operation_id = sequence.next();
    if let Ok(mut subscriber) = subscribers.get_mut(target) {
        subscriber.begin_operation(operation_id, kind);
    } else {
        commands
            .entity(target)
            .insert(VaultSubscriber::pending(operation_id, kind));
    }
    let mut connecting = false;
    for (entity, task) in &active_github {
        connecting = true;
        if let Some(task) = task {
            task.canceled.store(true, Ordering::Relaxed);
        } else {
            commands.entity(entity).despawn();
        }
    }
    if connecting {
        for entity in &pending {
            commands.entity(entity).despawn();
        }
    } else if kind == VaultOperationKind::ConnectGithub {
        for entity in &pending_github {
            commands.entity(entity).despawn();
        }
    }
    commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind,
        },
        VaultOperationRequest::new(request),
    ));
}

macro_rules! vault_operation_observer {
    ($name:ident, $request:ty, $kind:expr) => {
        fn $name(
            trigger: On<UiInput<$request>>,
            registries: Query<&mut OperationRequestSequence, With<ToolRegistry>>,
            pending: Query<Entity, With<PendingVaultOperation>>,
            pending_github: Query<
                Entity,
                (
                    With<PendingVaultOperation>,
                    With<VaultOperationRequest<VaultConnectGithubRequest>>,
                ),
            >,
            active_github: Query<
                (Entity, Option<&VaultOperationTask>),
                (
                    With<VaultOperationRequest<VaultConnectGithubRequest>>,
                    Without<PendingVaultOperation>,
                ),
            >,
            subscribers: Query<&mut VaultSubscriber>,
            commands: Commands,
        ) {
            queue_vault_operation(
                trigger.event().webview,
                trigger.event().payload.clone(),
                $kind,
                registries,
                pending,
                pending_github,
                active_github,
                subscribers,
                commands,
            );
        }
    };
}

vault_operation_observer!(
    on_vault_create_request,
    VaultCreateRequest,
    VaultOperationKind::Create
);
vault_operation_observer!(
    on_vault_connect_request,
    VaultConnectRequest,
    VaultOperationKind::Connect
);
vault_operation_observer!(
    on_vault_sync_request,
    VaultSyncRequest,
    VaultOperationKind::Sync
);
vault_operation_observer!(
    on_vault_connect_github_request,
    VaultConnectGithubRequest,
    VaultOperationKind::ConnectGithub
);
vault_operation_observer!(
    on_vault_connect_folder_request,
    VaultConnectFolderRequest,
    VaultOperationKind::ConnectFolder
);
vault_operation_observer!(
    on_vault_generate_recovery_key_request,
    VaultGenerateRecoveryKeyRequest,
    VaultOperationKind::GenerateRecoveryKey
);
vault_operation_observer!(
    on_vault_create_recovery_key_request,
    VaultCreateRecoveryKeyRequest,
    VaultOperationKind::CreateRecoveryKey
);
vault_operation_observer!(
    on_vault_unlock_recovery_key_request,
    VaultUnlockRecoveryKeyRequest,
    VaultOperationKind::UnlockRecoveryKey
);
vault_operation_observer!(
    on_vault_connect_cloud_request,
    VaultConnectCloudRequest,
    VaultOperationKind::ConnectCloud
);
vault_operation_observer!(
    on_vault_create_cloud_folder_request,
    VaultCreateCloudFolderRequest,
    VaultOperationKind::CreateCloudFolder
);
vault_operation_observer!(
    on_vault_choose_cloud_folder_request,
    VaultChooseCloudFolderRequest,
    VaultOperationKind::ChooseCloudFolder
);

fn on_vault_refresh_request(
    trigger: On<UiInput<VaultRefreshRequest>>,
    mut registry: Query<&mut ToolRegistry>,
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
    state.full_scan |= !state.snapshot.loaded;
    state.load_vault_repositories |= trigger.event().payload.load_repositories;
    state.generation = state.generation.wrapping_add(1);
    if state.snapshot.loaded {
        state.snapshot.loaded = false;
        state.revision = state.revision.wrapping_add(1);
    }
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

fn drain_vault_watch(
    watcher: Option<NonSendMut<VaultWatch>>,
    mut registry: Query<(&mut ToolRegistry, &mut VaultAutoSync)>,
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
        &ToolRegistry,
        &mut VaultAutoSync,
        &mut OperationRequestSequence,
    )>,
    scans: Query<(), With<ToolsScanTask>>,
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
    if !auto_sync.requested || state.dirty || !state.snapshot.loaded || !scans.is_empty() {
        return;
    }
    let vault = &state.snapshot.vault;
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

fn start_tool_operation(
    pending: Query<(Entity, &ToolOperationContext), With<PendingToolOperation>>,
    active: Query<(), With<ToolStoreTarget>>,
    vault_tasks: Query<(), With<VaultOperationTask>>,
    vault_ready: Query<(), With<ReadyVaultOperation>>,
    scans: Query<(), With<ToolsScanTask>>,
    stores: Query<Entity, (With<ToolStore>, With<ToolRegistry>)>,
    mut commands: Commands,
) {
    if !active.is_empty() || !vault_tasks.is_empty() || !vault_ready.is_empty() || !scans.is_empty()
    {
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
    let Ok(store) = stores.single() else {
        return;
    };
    commands
        .entity(entity)
        .remove::<PendingToolOperation>()
        .insert(ToolStoreTarget::new(store));
}

macro_rules! external_tool_system {
    ($name:ident, $request:ty, $execute:ident) => {
        fn $name(
            operations: Query<
                (Entity, &ToolOperationRequest<$request>, &ToolStoreTarget),
                Added<ExternalToolOperation>,
            >,
            stores: Query<&ToolStore>,
            mut commands: Commands,
        ) {
            for (entity, operation, target) in &operations {
                let Ok(store) = stores.get(target.entity()).cloned() else {
                    continue;
                };
                let request = operation.request().clone();
                let task = IoTaskPool::get().spawn(async move { $execute(request, &store) });
                commands
                    .entity(entity)
                    .remove::<ExternalToolOperation>()
                    .insert(ToolOperationTask { task });
            }
        }
    };
}

external_tool_system!(start_tool_install, ToolInstallRequest, install_tool);
external_tool_system!(start_tool_update, ToolUpdateRequest, update_tool);
external_tool_system!(start_tool_uninstall, ToolUninstallRequest, uninstall_tool);
external_tool_system!(start_tool_forget, ToolForgetRequest, forget_tool);
external_tool_system!(start_tool_adopt, ToolAdoptRequest, adopt_tool);
external_tool_system!(start_tool_link, ToolLinkRequest, link_tool);
external_tool_system!(start_tool_unlink, ToolUnlinkRequest, unlink_tool);
external_tool_system!(start_tool_apply, ToolApplyRequest, apply_tools);
external_tool_system!(start_tool_import, ToolImportRequest, import_tools);

fn start_vault_operation(
    pending: Query<(Entity, &VaultOperationContext), With<PendingVaultOperation>>,
    tasks: Query<(), With<VaultOperationTask>>,
    ready: Query<(), With<ReadyVaultOperation>>,
    tool_operations: Query<(), With<ToolStoreTarget>>,
    scans: Query<(), With<ToolsScanTask>>,
    mut commands: Commands,
) {
    if !tasks.is_empty() || !ready.is_empty() || !tool_operations.is_empty() || !scans.is_empty() {
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

macro_rules! vault_launch_system {
    ($name:ident, $request:ty, $execute:ident, $take_key:expr) => {
        fn $name(
            operations: Query<
                (Entity, &VaultOperationRequest<$request>),
                Added<ReadyVaultOperation>,
            >,
            mut registries: Query<&mut VaultRecoveryState, With<ToolRegistry>>,
            proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
            mut commands: Commands,
        ) {
            let Ok(mut recovery) = registries.single_mut() else {
                return;
            };
            for (entity, operation) in &operations {
                let request = operation.request().clone();
                let service = recovery.service();
                let generated_recovery_key = if $take_key {
                    recovery.take_pending_key()
                } else {
                    None
                };
                let completion_wake = proxy.as_deref().map(|proxy| (**proxy).clone());
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
                let operation = $execute(
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
                commands
                    .entity(entity)
                    .remove::<ReadyVaultOperation>()
                    .insert(VaultOperationTask {
                        task,
                        progress: Mutex::new(progress_receiver),
                        canceled,
                    });
            }
        }
    };
}

vault_launch_system!(launch_vault_create, VaultCreateRequest, create_vault, false);
vault_launch_system!(
    launch_vault_connect,
    VaultConnectRequest,
    connect_vault,
    false
);
vault_launch_system!(launch_vault_sync, VaultSyncRequest, sync_vault, false);
vault_launch_system!(
    launch_vault_connect_github,
    VaultConnectGithubRequest,
    connect_vault_github,
    false
);
vault_launch_system!(
    launch_vault_connect_folder,
    VaultConnectFolderRequest,
    connect_vault_folder,
    false
);
vault_launch_system!(
    launch_vault_generate_recovery_key,
    VaultGenerateRecoveryKeyRequest,
    generate_vault_recovery_key,
    false
);
vault_launch_system!(
    launch_vault_create_recovery_key,
    VaultCreateRecoveryKeyRequest,
    create_vault_recovery_key,
    true
);
vault_launch_system!(
    launch_vault_unlock_recovery_key,
    VaultUnlockRecoveryKeyRequest,
    unlock_vault_recovery_key,
    false
);
vault_launch_system!(
    launch_vault_connect_cloud,
    VaultConnectCloudRequest,
    connect_vault_cloud,
    false
);
vault_launch_system!(
    launch_vault_create_cloud_folder,
    VaultCreateCloudFolderRequest,
    create_vault_cloud_folder,
    false
);
vault_launch_system!(
    launch_vault_choose_cloud_folder,
    VaultChooseCloudFolderRequest,
    choose_vault_cloud_folder,
    false
);

fn start_tools_scan(
    mut registry: Query<(&mut ToolRegistry, &ToolStore)>,
    tasks: Query<(), With<ToolsScanTask>>,
    tool_operations: Query<(), With<ToolStoreTarget>>,
    vault_tasks: Query<(), With<VaultOperationTask>>,
    vault_ready: Query<(), With<ReadyVaultOperation>>,
    pending_tool_operations: Query<(), With<PendingToolOperation>>,
    pending_vault_operations: Query<(), With<PendingVaultOperation>>,
    mut commands: Commands,
) {
    let Ok((mut state, store)) = registry.single_mut() else {
        return;
    };
    if !state.dirty
        || !tasks.is_empty()
        || !tool_operations.is_empty()
        || !vault_tasks.is_empty()
        || !vault_ready.is_empty()
        || !pending_tool_operations.is_empty()
        || !pending_vault_operations.is_empty()
    {
        return;
    }
    let generation = state.generation;
    let full_scan = state.full_scan;
    let refresh_catalogs = state.refresh_catalogs;
    let load_vault_repositories = state.load_vault_repositories;
    let previous_snapshot = state.snapshot.clone();
    let store = store.clone();
    state.dirty = false;
    state.full_scan = false;
    state.refresh_catalogs = false;
    state.load_vault_repositories = false;
    let task = IoTaskPool::get().spawn(async move {
        if full_scan {
            scan_tools(
                &store,
                refresh_catalogs,
                load_vault_repositories,
                previous_snapshot.vault,
            )
        } else {
            let mut snapshot = previous_snapshot;
            snapshot.vault = scan_vault(load_vault_repositories, snapshot.vault);
            let manifest = store.load().unwrap_or_default();
            ToolsScanOutput { snapshot, manifest }
        }
    });
    commands.spawn(ToolsScanTask { generation, task });
}

fn drain_tools_scan(
    mut tasks: Query<(Entity, &mut ToolsScanTask)>,
    mut registry: Query<(&mut ToolRegistry, &mut ToolsManifest, &mut VaultAutoSync)>,
    mut commands: Commands,
) {
    let Ok((mut state, mut manifest, mut auto_sync)) = registry.single_mut() else {
        return;
    };
    for (entity, mut task) in &mut tasks {
        let Some(snapshot) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if task.generation != state.generation {
            state.dirty = true;
            continue;
        }
        state.snapshot = snapshot.snapshot;
        *manifest = snapshot.manifest;
        state.revision = state.revision.wrapping_add(1);
        let vault = &state.snapshot.vault;
        if !vault.github_owner.is_empty()
            && (!vault.initialized || vault.remote.is_empty())
            && !vault.repositories_loaded
        {
            state.dirty = true;
            state.load_vault_repositories = true;
            state.generation = state.generation.wrapping_add(1);
        }
        if !auto_sync.initial_scan_complete {
            let vault = &state.snapshot.vault;
            auto_sync.requested = vault.initialized
                && vault.unlocked
                && !vault.remote.is_empty()
                && (vault.dirty > 0 || vault.ahead > 0 || vault.behind > 0);
            auto_sync.initial_scan_complete = true;
        }
    }
}

fn drain_tool_operations(
    mut tasks: Query<(Entity, &ToolOperationContext, &mut ToolOperationTask)>,
    mut registry: Query<&mut ToolRegistry>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut commands: Commands,
) {
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    for (entity, operation, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let (success, message) = match result {
            Ok(message) => (true, message),
            Err(message) => (false, message),
        };
        if let Ok(mut subscriber) = subscribers.get_mut(operation.target) {
            subscriber.complete(
                operation.operation_id,
                operation.operation.clone(),
                success,
                message,
            );
        }
        if success {
            state.dirty = true;
            state.full_scan = true;
            state.generation = state.generation.wrapping_add(1);
        }
    }
}

fn drain_tool_store_operations(
    operations: Query<
        (Entity, &ToolOperationContext, &ToolOperationCompletion),
        With<ToolStoreOperation>,
    >,
    mut registry: Query<&mut ToolRegistry>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut commands: Commands,
) {
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    for (entity, operation, completion) in &operations {
        if let Ok(mut subscriber) = subscribers.get_mut(operation.target) {
            subscriber.complete(
                operation.operation_id,
                operation.operation.clone(),
                completion.success(),
                completion.message().to_string(),
            );
        }
        if completion.success() {
            state.dirty = true;
            state.full_scan = true;
            state.generation = state.generation.wrapping_add(1);
        }
        commands.entity(entity).despawn();
    }
}

fn drain_vault_operations(
    mut operations: Query<(Entity, &VaultOperationContext, &mut VaultOperationTask)>,
    mut registry: Query<(&mut ToolRegistry, &mut VaultRecoveryState)>,
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
            state.snapshot.vault.sync_failed = !success;
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
        state.full_scan |= !state.snapshot.loaded;
        state.load_vault_repositories |= context.kind == VaultOperationKind::ConnectGithub;
        state.generation = state.generation.wrapping_add(1);
    }
}

fn emit_tools_state(
    registry: Query<&ToolRegistry>,
    mut subscribers: Query<(Entity, &mut ToolSubscriber)>,
    mut commands: Commands,
) {
    let Ok(state) = registry.single() else {
        return;
    };
    for (entity, mut subscriber) in &mut subscribers {
        subscriber.synchronize(state.revision, &state.snapshot);
        if subscriber.emitted_revision == subscriber.revision {
            continue;
        }
        commands.trigger(UiStateWrite::<ToolsUiState>::from_event(
            entity,
            &subscriber.state,
        ));
        subscriber.emitted_revision = subscriber.revision;
    }
}

fn emit_vault_state(
    registry: Query<&ToolRegistry>,
    mut subscribers: Query<(Entity, &mut VaultSubscriber, &mut VaultWorkflow)>,
    mut commands: Commands,
) {
    let Ok(state) = registry.single() else {
        return;
    };
    for (entity, mut subscriber, mut workflow) in &mut subscribers {
        let snapshot_changed = subscriber.synchronize(state.revision, &state.snapshot.vault);
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

fn scan_tools(
    store: &ToolStore,
    refresh_catalogs: bool,
    load_vault_repositories: bool,
    previous_vault: VaultSnapshot,
) -> ToolsScanOutput {
    let (mut manifest, manifest_error) = match store.load() {
        Ok(manifest) => (manifest, None),
        Err(error) => (ToolsManifest::default(), Some(error)),
    };
    let can_persist = manifest_error.is_none();
    let original_manifest = manifest.clone();
    let mut categories = Vec::new();
    let mut errors = manifest_error.into_iter().collect::<Vec<_>>();
    let providers = [
        (
            ToolProvider::HomebrewFormula,
            scan_homebrew(false, refresh_catalogs),
        ),
        (
            ToolProvider::HomebrewCask,
            scan_homebrew(true, refresh_catalogs),
        ),
        (ToolProvider::Npm, scan_npm(refresh_catalogs)),
        (ToolProvider::Acp, scan_acp(refresh_catalogs)),
        (ToolProvider::Lsp, scan_lsp(refresh_catalogs)),
    ];
    let mut inventories = Vec::new();
    for (provider, result) in providers {
        let inventory = match result {
            Ok(inventory) => inventory,
            Err(error) => {
                errors.push(format!("{}: {error}", provider.title()));
                Vec::new()
            }
        };
        import_inventory(&mut manifest, provider, inventory.clone());
        inventories.push((provider, inventory));
    }
    categories.extend(
        inventories
            .into_iter()
            .map(|(provider, inventory)| build_category(provider, inventory, &manifest)),
    );
    categories.push(scan_mcp(store, &mut manifest, &mut errors));
    categories.push(scan_dotfiles(store, &mut manifest));
    if can_persist
        && manifest != original_manifest
        && let Err(error) = store.save(&manifest)
    {
        errors.push(format!("Tools: {error}"));
    }
    let installed = categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| matches!(item.status, ToolStatus::Installed | ToolStatus::Outdated))
        .count() as u32;
    let updates = categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item.status == ToolStatus::Outdated)
        .count() as u32;
    let conflicts = categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item.status == ToolStatus::Conflict)
        .count() as u32;
    ToolsScanOutput {
        snapshot: ToolsSnapshot {
            loaded: true,
            root: store.root().to_string_lossy().into_owned(),
            vault: scan_vault(load_vault_repositories, previous_vault),
            categories,
            installed,
            updates,
            conflicts,
            error: errors.join("\n"),
        },
        manifest,
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

fn build_category(
    provider: ToolProvider,
    inventory: Vec<InventoryItem>,
    manifest: &ToolsManifest,
) -> ToolCategory {
    let mut items = inventory
        .into_iter()
        .map(|item| {
            let managed = manifest.contains(provider.id(), &item.id);
            ToolItem {
                provider,
                operations: package_operations(item.status, managed, item.removable),
                id: item.id,
                name: item.name,
                icon: item.icon,
                version: item.version,
                detail: item.detail,
                status: item.status,
                managed,
            }
        })
        .collect::<Vec<_>>();
    let existing = items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    for name in manifest.managed_packages(provider.id()) {
        if !existing.contains(&name) {
            items.push(ToolItem {
                provider,
                id: name.clone(),
                name,
                icon: None,
                version: None,
                detail: "Declared in tools.toml".to_string(),
                status: ToolStatus::Missing,
                managed: true,
                operations: vec![ToolOperationKind::Install, ToolOperationKind::Forget],
            });
        }
    }
    items.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    ToolCategory { provider, items }
}

fn package_operations(
    status: ToolStatus,
    managed: bool,
    removable: bool,
) -> Vec<ToolOperationKind> {
    let mut operations = Vec::new();
    if !managed && matches!(status, ToolStatus::Installed | ToolStatus::Outdated) {
        operations.push(ToolOperationKind::Adopt);
    }
    if status == ToolStatus::Outdated {
        operations.push(ToolOperationKind::Update);
    }
    if status == ToolStatus::Missing {
        operations.push(ToolOperationKind::Install);
    }
    if removable {
        operations.push(ToolOperationKind::Uninstall);
    }
    operations
}

fn scan_homebrew(cask: bool, refresh: bool) -> Result<Vec<InventoryItem>, String> {
    if vmux_agent::exec::find_executable("brew").is_none() {
        return Ok(Vec::new());
    }
    let mut args = vec!["list"];
    args.push(if cask { "--cask" } else { "--formula" });
    args.push("--versions");
    let output = command_output("brew", &args, true)?;
    let outdated = if refresh {
        let mut outdated_args = vec!["outdated"];
        outdated_args.push(if cask { "--cask" } else { "--formula" });
        command_output("brew", &outdated_args, false)
            .map(|output| parse_name_lines(&output.stdout))
            .unwrap_or_default()
    } else {
        BTreeSet::new()
    };
    Ok(parse_brew_versions(&output.stdout)
        .into_iter()
        .map(|(name, version)| {
            let status = if outdated.contains(&name) {
                ToolStatus::Outdated
            } else {
                ToolStatus::Installed
            };
            let removable = !cask || name != "vmux";
            InventoryItem {
                id: name.clone(),
                name,
                icon: None,
                version,
                detail: if cask {
                    "Homebrew cask".to_string()
                } else {
                    "Homebrew formula".to_string()
                },
                status,
                removable,
            }
        })
        .collect())
}

fn parse_brew_versions(bytes: &[u8]) -> Vec<(String, Option<String>)> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?.to_string();
            let version = fields.collect::<Vec<_>>().join(" ");
            Some((name, (!version.is_empty()).then_some(version)))
        })
        .collect()
}

fn parse_name_lines(bytes: &[u8]) -> BTreeSet<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}

fn scan_npm(refresh: bool) -> Result<Vec<InventoryItem>, String> {
    if vmux_agent::exec::find_executable("npm").is_none() {
        return Ok(Vec::new());
    }
    let output = command_output("npm", &["list", "--global", "--depth=0", "--json"], false)?;
    if output.stdout.is_empty() && !output.status.success() {
        return Err(command_error("npm", &output));
    }
    let outdated_output = refresh
        .then(|| command_output("npm", &["outdated", "--global", "--json"], false).ok())
        .flatten();
    let outdated = outdated_output
        .as_ref()
        .and_then(|output| serde_json::from_slice::<serde_json::Value>(&output.stdout).ok())
        .and_then(|value| {
            value
                .as_object()
                .map(|packages| packages.keys().cloned().collect())
        })
        .unwrap_or_default();
    parse_npm_inventory(&output.stdout, &outdated)
}

fn parse_npm_inventory(
    bytes: &[u8],
    outdated: &BTreeSet<String>,
) -> Result<Vec<InventoryItem>, String> {
    let document: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let dependencies = document
        .get("dependencies")
        .and_then(|dependencies| dependencies.as_object())
        .cloned()
        .unwrap_or_default();
    Ok(dependencies
        .into_iter()
        .map(|(name, metadata)| {
            let status = if outdated.contains(&name) {
                ToolStatus::Outdated
            } else {
                ToolStatus::Installed
            };
            InventoryItem {
                id: name.clone(),
                name,
                icon: None,
                version: metadata
                    .get("version")
                    .and_then(|version| version.as_str())
                    .map(str::to_string),
                detail: "Global NPM package".to_string(),
                status,
                removable: true,
            }
        })
        .collect())
}

fn scan_acp(refresh: bool) -> Result<Vec<InventoryItem>, String> {
    let catalog = if refresh {
        vmux_agent::acp_registry::fetch_blocking()
            .ok()
            .or_else(vmux_agent::acp_registry::load_cached)
    } else {
        vmux_agent::acp_registry::load_cached()
    };
    let catalog = catalog
        .map(|registry| {
            registry
                .agents
                .into_iter()
                .map(|agent| (agent.id.clone(), agent))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let receipts = vmux_editor::lsp::store::installed(&vmux_agent::acp_registry::agents_dir());
    Ok(receipts
        .into_values()
        .filter(|receipt| receipt.source_id.starts_with("acp:"))
        .map(|receipt| {
            let agent = catalog.get(receipt.name.as_str());
            let latest = agent.and_then(|agent| agent.version.clone());
            InventoryItem {
                id: receipt.name.as_str().to_string(),
                name: agent
                    .map(|agent| agent.name.clone())
                    .unwrap_or_else(|| receipt.name.as_str().to_string()),
                icon: agent.and_then(|agent| agent.icon.clone()),
                version: receipt.version.clone(),
                detail: agent
                    .and_then(|agent| agent.description.clone())
                    .unwrap_or_else(|| "ACP agent".to_string()),
                status: if receipt.version.is_some()
                    && latest.is_some()
                    && receipt.version != latest
                {
                    ToolStatus::Outdated
                } else {
                    ToolStatus::Installed
                },
                removable: true,
            }
        })
        .collect())
}

fn scan_lsp(refresh: bool) -> Result<Vec<InventoryItem>, String> {
    let root = vmux_editor::lsp::store::default_root();
    let catalog = if refresh {
        vmux_editor::lsp::catalog::ensure_catalog(&root, true).unwrap_or_default()
    } else if vmux_editor::lsp::catalog::cached_path(&root).is_file() {
        let source = std::fs::read_to_string(vmux_editor::lsp::catalog::cached_path(&root))
            .map_err(|error| error.to_string())?;
        vmux_editor::lsp::catalog::parse_registry(&source).unwrap_or_default()
    } else {
        Vec::new()
    };
    let catalog_by_name = catalog
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect::<BTreeMap<_, _>>();
    let receipts = vmux_editor::lsp::store::installed(&root);
    let mut inventory = receipts
        .into_values()
        .map(|receipt| {
            let package = catalog_by_name.get(&receipt.name).copied();
            let latest = package
                .and_then(|package| vmux_editor::lsp::purl::parse(&package.source_id))
                .and_then(|purl| purl.version);
            InventoryItem {
                id: receipt.name.as_str().to_string(),
                name: receipt.name.as_str().to_string(),
                icon: None,
                version: receipt.version.clone(),
                detail: package
                    .map(|package| package.description.clone())
                    .filter(|detail| !detail.is_empty())
                    .unwrap_or_else(|| "Vmux-managed language tool".to_string()),
                status: if receipt.version.is_some()
                    && latest.is_some()
                    && receipt.version != latest
                {
                    ToolStatus::Outdated
                } else {
                    ToolStatus::Installed
                },
                removable: true,
            }
        })
        .collect::<Vec<_>>();
    let installed = inventory
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    for package in catalog {
        if installed.contains(package.name.as_str()) {
            continue;
        }
        let on_path = package.bin.keys().any(|command| {
            matches!(
                vmux_editor::lsp::store::resolved_command(&root, command.as_str()),
                vmux_editor::lsp::store::Resolution::OnPath
            )
        });
        if on_path {
            inventory.push(InventoryItem {
                id: package.name.as_str().to_string(),
                name: package.name.as_str().to_string(),
                icon: None,
                version: None,
                detail: "Available on PATH".to_string(),
                status: ToolStatus::Installed,
                removable: false,
            });
        }
    }
    Ok(inventory)
}

fn scan_mcp(
    store: &ToolStore,
    manifest: &mut ToolsManifest,
    errors: &mut Vec<String>,
) -> ToolCategory {
    let (discovered, discovery_errors) = vmux_tool::discover_mcp_servers_at(store.home());
    errors.extend(
        discovery_errors
            .into_iter()
            .map(|error| format!("MCP Servers: {error}")),
    );
    for (name, server) in &discovered {
        if name != "vmux" && name != "linear" && !server.conflict {
            manifest
                .mcp
                .servers
                .entry(name.clone())
                .or_insert_with(|| server.definition.clone());
        }
    }
    let mut names = discovered.keys().cloned().collect::<BTreeSet<_>>();
    names.extend(manifest.mcp.servers.keys().cloned());
    let items = names
        .into_iter()
        .map(|name| {
            let managed = manifest.mcp.servers.contains_key(&name);
            let external = discovered.get(&name);
            let status = if managed {
                ToolStatus::Installed
            } else if external.is_some_and(|server| server.conflict) {
                ToolStatus::Conflict
            } else {
                ToolStatus::Available
            };
            let definition = manifest
                .mcp
                .servers
                .get(&name)
                .or_else(|| external.map(|server| &server.definition));
            let transport = definition
                .map(|server| format!("{:?}", server.transport).to_ascii_lowercase())
                .unwrap_or_else(|| "unknown".to_string());
            let sources = external
                .map(|server| {
                    server
                        .sources
                        .iter()
                        .map(|path| path.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let detail = if external.is_some_and(|server| server.conflict) && !managed {
                format!("Conflicting definitions in {sources}")
            } else if managed && sources.is_empty() {
                format!("{transport} · Tools managed")
            } else if managed {
                format!("{transport} · Tools managed · imported from {sources}")
            } else {
                format!("{transport} · configured in {sources}")
            };
            let operations = if managed {
                vec![ToolOperationKind::Forget]
            } else if status == ToolStatus::Available {
                vec![ToolOperationKind::Adopt]
            } else {
                Vec::new()
            };
            ToolItem {
                provider: ToolProvider::Mcp,
                id: name.clone(),
                name,
                icon: None,
                version: None,
                detail,
                status,
                managed,
                operations,
            }
        })
        .collect();
    ToolCategory {
        provider: ToolProvider::Mcp,
        items,
    }
}

fn scan_dotfiles(store: &ToolStore, manifest: &mut ToolsManifest) -> ToolCategory {
    let discovered = vmux_tool::dotfile_packages_in(&store.dotfiles_dir());
    for package in &discovered {
        manifest.set_dotfile_package(package, true);
    }
    let mut package_names = discovered.into_iter().collect::<BTreeSet<_>>();
    package_names.extend(manifest.dotfiles.packages.iter().cloned());
    let mut items = Vec::new();
    for package in package_names {
        let managed = manifest.dotfiles.packages.contains(&package);
        let (status, detail, operations) =
            match vmux_tool::plan_dotfile_package_in(&store.dotfiles_dir(), store.home(), &package)
            {
                Ok(plan) => {
                    let detail = format!(
                        "{} linked · {} missing · {} conflicts",
                        plan.linked(),
                        plan.missing(),
                        plan.conflicts()
                    );
                    let status = if plan.conflicts() > 0 {
                        ToolStatus::Conflict
                    } else if plan.missing() > 0 {
                        if managed {
                            ToolStatus::Missing
                        } else {
                            ToolStatus::Available
                        }
                    } else {
                        ToolStatus::Installed
                    };
                    let operations = if managed {
                        vec![ToolOperationKind::Link, ToolOperationKind::Unlink]
                    } else {
                        vec![ToolOperationKind::Link]
                    };
                    (status, detail, operations)
                }
                Err(error) => (
                    ToolStatus::Missing,
                    error,
                    if managed {
                        vec![ToolOperationKind::Unlink]
                    } else {
                        Vec::new()
                    },
                ),
            };
        items.push(ToolItem {
            provider: ToolProvider::Dotfiles,
            id: package.clone(),
            name: package,
            icon: None,
            version: None,
            detail,
            status,
            managed,
            operations,
        });
    }
    ToolCategory {
        provider: ToolProvider::Dotfiles,
        items,
    }
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

fn import_provider(
    store: &ToolStore,
    provider: ToolProvider,
    path: &str,
) -> Result<String, String> {
    match provider {
        ToolProvider::HomebrewFormula | ToolProvider::HomebrewCask => {
            if !path.is_empty() {
                Err("Brewfile import request reached the desktop fallback".to_string())
            } else {
                let formulae = scan_homebrew(false, false)?;
                let casks = scan_homebrew(true, false)?;
                let mut manifest = store.load()?;
                let formulae =
                    import_inventory(&mut manifest, ToolProvider::HomebrewFormula, formulae);
                let casks = import_inventory(&mut manifest, ToolProvider::HomebrewCask, casks);
                store.save(&manifest)?;
                Ok(format!("imported {formulae} formulae and {casks} casks"))
            }
        }
        ToolProvider::Npm => {
            if !path.is_empty() {
                Err("NPM manifest import request reached the desktop fallback".to_string())
            } else {
                import_scanned_inventory(store, provider, scan_npm(false)?)
            }
        }
        ToolProvider::Acp => import_scanned_inventory(store, provider, scan_acp(false)?),
        ToolProvider::Lsp => import_scanned_inventory(store, provider, scan_lsp(false)?),
        ToolProvider::Mcp => Err("MCP import request reached the desktop fallback".to_string()),
        ToolProvider::Dotfiles => {
            Err("dotfile import request reached the desktop fallback".to_string())
        }
    }
}

fn import_scanned_inventory(
    store: &ToolStore,
    provider: ToolProvider,
    inventory: Vec<InventoryItem>,
) -> Result<String, String> {
    let mut manifest = store.load()?;
    let imported = import_inventory(&mut manifest, provider, inventory);
    store.save(&manifest)?;
    Ok(format!("imported {imported} {} item(s)", provider.id()))
}

fn import_inventory(
    manifest: &mut ToolsManifest,
    provider: ToolProvider,
    inventory: Vec<InventoryItem>,
) -> usize {
    let mut imported = 0;
    for item in inventory
        .into_iter()
        .filter(|item| matches!(item.status, ToolStatus::Installed | ToolStatus::Outdated))
    {
        imported += usize::from(!manifest.contains(provider.id(), &item.id));
        manifest.set_package(provider.id(), &item.id, true);
    }
    imported
}

fn apply_manifest(store: &ToolStore) -> Result<String, String> {
    let manifest = store.load()?;
    let snapshot = scan_tools(store, false, false, VaultSnapshot::default()).snapshot;
    let mut installed = 0;
    for item in snapshot
        .categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item.managed && item.status == ToolStatus::Missing)
        .filter(|item| !matches!(item.provider, ToolProvider::Dotfiles | ToolProvider::Mcp))
    {
        install_provider(store, item.provider, &item.id)?;
        installed += 1;
    }
    let linked =
        vmux_tool::apply_enabled_dotfiles_in(&manifest, &store.dotfiles_dir(), store.home())?;
    Ok(format!(
        "installed {installed} package(s), linked {linked} file(s)"
    ))
}

fn set_manifest_entry(
    store: &ToolStore,
    provider: ToolProvider,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut manifest = store.load()?;
    if provider == ToolProvider::Dotfiles {
        manifest.set_dotfile_package(id, enabled);
    } else if provider == ToolProvider::Mcp {
        if enabled {
            return Err("MCP servers must be imported from a config".to_string());
        }
        manifest.mcp.servers.remove(id);
    } else {
        manifest.set_package(provider.id(), id, enabled);
    }
    store.save(&manifest)
}

fn install_provider(store: &ToolStore, provider: ToolProvider, id: &str) -> Result<(), String> {
    match provider {
        ToolProvider::HomebrewFormula => {
            command_output("brew", &["install", id], true)?;
        }
        ToolProvider::HomebrewCask => {
            command_output("brew", &["install", "--cask", id], true)?;
        }
        ToolProvider::Npm => {
            command_output("npm", &["install", "--global", id], true)?;
        }
        ToolProvider::Acp => {
            vmux_agent::acp_tool::resolve_from_registry(id, None, |_, _, _| {})?;
        }
        ToolProvider::Lsp => {
            let root = vmux_editor::lsp::store::default_root();
            let packages = vmux_editor::lsp::catalog::ensure_catalog(&root, false)?;
            let package = packages
                .iter()
                .find(|package| package.name.as_str() == id)
                .ok_or_else(|| format!("language tool not found: {id}"))?;
            vmux_editor::lsp::install::install(
                package,
                &root,
                vmux_editor::lsp::target::host_target(),
                |_, _, _| {},
            )?;
        }
        ToolProvider::Dotfiles => {
            vmux_tool::apply_dotfile_package_in(&store.dotfiles_dir(), store.home(), id)?;
        }
        ToolProvider::Mcp => return Err("MCP servers are configuration, not packages".into()),
    }
    Ok(())
}

fn uninstall_provider(store: &ToolStore, provider: ToolProvider, id: &str) -> Result<(), String> {
    match provider {
        ToolProvider::HomebrewFormula => {
            command_output("brew", &["uninstall", id], true)?;
        }
        ToolProvider::HomebrewCask => {
            command_output("brew", &["uninstall", "--cask", id], true)?;
        }
        ToolProvider::Npm => {
            command_output("npm", &["uninstall", "--global", id], true)?;
        }
        ToolProvider::Acp => vmux_agent::acp_tool::uninstall(id)?,
        ToolProvider::Lsp => {
            let name = vmux_editor::lsp::package_path::PackageName::parse(id)?;
            vmux_editor::lsp::store::remove(&vmux_editor::lsp::store::default_root(), &name)
                .map_err(|error| error.to_string())?;
        }
        ToolProvider::Dotfiles => {
            let _ = store.load()?;
            vmux_tool::unlink_dotfile_package_in(&store.dotfiles_dir(), store.home(), id)?;
        }
        ToolProvider::Mcp => return Err("forget the MCP server instead".to_string()),
    }
    Ok(())
}

fn update_provider(store: &ToolStore, provider: ToolProvider, id: &str) -> Result<(), String> {
    match provider {
        ToolProvider::HomebrewFormula => {
            command_output("brew", &["upgrade", id], true)?;
        }
        ToolProvider::HomebrewCask => {
            command_output("brew", &["upgrade", "--cask", id], true)?;
        }
        ToolProvider::Npm => {
            command_output("npm", &["update", "--global", id], true)?;
        }
        ToolProvider::Acp | ToolProvider::Lsp | ToolProvider::Dotfiles => {
            install_provider(store, provider, id)?;
        }
        ToolProvider::Mcp => return Err("MCP servers do not update through Tools".into()),
    }
    Ok(())
}

fn command_output(program: &str, args: &[&str], require_success: bool) -> Result<Output, String> {
    let executable = vmux_agent::exec::find_executable(program)
        .ok_or_else(|| format!("{program} is not installed"))?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let output = Command::new(executable)
        .args(args)
        .envs(
            vmux_terminal::shell_env::login_shell_env(&shell)
                .iter()
                .cloned(),
        )
        .output()
        .map_err(|error| error.to_string())?;
    if require_success && !output.status.success() {
        return Err(command_error(program, &output));
    }
    Ok(output)
}

fn command_error(program: &str, output: &Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        format!("{program} exited with {}", output.status)
    } else {
        detail
    }
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
                    ToolRegistry::default(),
                    VaultAutoSync::default(),
                    OperationRequestSequence::default(),
                ))
                .id();
            {
                let mut state = app.world_mut().get_mut::<ToolRegistry>(registry).unwrap();
                state.dirty = false;
                state.snapshot.loaded = true;
                state.snapshot.vault = vault;
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
    fn tools_page_owns_provider_routes() {
        let hosted = vmux_core::host::page::NativelyHosted::subtree(
            vmux_layout::tool_page::ToolsPage::URL,
            vmux_layout::tool_page::ToolsPage::NATIVE.title,
        );
        assert!(hosted.answers_for("vmux://tools/extensions"));
        assert!(hosted.answers_for("vmux://tools/homebrew"));
        assert!(!hosted.answers_for("vmux://toolbox/"));
    }

    #[test]
    fn recovery_key_is_consumed_once() {
        let mut recovery = VaultRecoveryState::default();
        recovery.retain(GeneratedRecoveryKey::generate().unwrap());

        assert!(recovery.take_pending_key().is_some());
        assert!(recovery.take_pending_key().is_none());
    }

    #[test]
    fn tool_operation_state_tracks_pending_and_completion() {
        let operation =
            ToolOperationKey::new(ToolProvider::Npm, ToolOperationKind::Install, "typescript");
        let mut subscriber = ToolSubscriber::pending(7, operation.clone());

        assert_eq!(
            subscriber.state.pending,
            vec![ToolOperationKey::new(
                ToolProvider::Npm,
                ToolOperationKind::Install,
                "typescript",
            )]
        );

        subscriber.complete(7, operation, true, "installed".to_string());

        assert!(subscriber.state.pending.is_empty());
        let notice = subscriber.state.notice.as_ref().unwrap();
        assert!(notice.success);
        assert_eq!(notice.message, "installed");
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
    fn tools_tab_navigation_replaces_the_owning_stack() {
        let mut app = App::new();
        app.add_message::<vmux_core::PageOpenRequest>()
            .add_observer(on_navigate_request);
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::Stack::default())
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ToolsNavigateRequest {
                url: "vmux://tools/lsp".to_string(),
            },
        });

        let messages = app
            .world()
            .resource::<Messages<vmux_core::PageOpenRequest>>();
        let mut cursor = messages.get_cursor();
        let requests = cursor.read(messages).collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "vmux://tools/lsp");
        assert!(matches!(
            requests[0].target,
            vmux_core::PageOpenTarget::Stack(target) if target == stack
        ));
    }

    #[test]
    fn parses_brew_inventory_with_versions() {
        assert_eq!(
            parse_brew_versions(b"ripgrep 14.1.1\nopenssl@3 3.5.0 3.5.1\n"),
            vec![
                ("ripgrep".to_string(), Some("14.1.1".to_string())),
                ("openssl@3".to_string(), Some("3.5.0 3.5.1".to_string())),
            ]
        );
    }

    #[test]
    fn category_adds_declared_missing_packages() {
        let mut manifest = ToolsManifest::default();
        manifest.set_package(ToolProvider::Npm.id(), "typescript", true);
        let category = build_category(ToolProvider::Npm, Vec::new(), &manifest);
        assert_eq!(category.items.len(), 1);
        assert_eq!(category.items[0].status, ToolStatus::Missing);
        assert!(category.items[0].managed);
        assert_eq!(
            category.items[0].operations,
            [ToolOperationKind::Install, ToolOperationKind::Forget]
        );
    }

    #[test]
    fn category_preserves_inventory_icon() {
        let category = build_category(
            ToolProvider::Acp,
            vec![InventoryItem {
                id: "codex-acp".to_string(),
                name: "Codex".to_string(),
                icon: Some("https://cdn.example/codex.svg".to_string()),
                version: Some("1.0.0".to_string()),
                detail: String::new(),
                status: ToolStatus::Installed,
                removable: true,
            }],
            &ToolsManifest::default(),
        );

        assert_eq!(
            category.items[0].icon.as_deref(),
            Some("https://cdn.example/codex.svg")
        );
    }

    #[test]
    fn parses_scoped_npm_packages_and_outdated_state() {
        let inventory = parse_npm_inventory(
        br#"{"dependencies":{"@scope/tool":{"version":"2.0.0"},"typescript":{"version":"5.9.0"}}}"#,
        &BTreeSet::from(["@scope/tool".to_string()]),
    )
    .unwrap();
        assert_eq!(inventory.len(), 2);
        let scoped = inventory
            .iter()
            .find(|item| item.id == "@scope/tool")
            .unwrap();
        assert_eq!(scoped.version.as_deref(), Some("2.0.0"));
        assert_eq!(scoped.status, ToolStatus::Outdated);
    }

    #[test]
    fn bulk_import_adopts_only_installed_inventory() {
        let mut manifest = ToolsManifest::default();
        let imported = import_inventory(
            &mut manifest,
            ToolProvider::Npm,
            vec![
                InventoryItem {
                    id: "installed".to_string(),
                    name: "installed".to_string(),
                    icon: None,
                    version: Some("1".to_string()),
                    detail: String::new(),
                    status: ToolStatus::Installed,
                    removable: true,
                },
                InventoryItem {
                    id: "missing".to_string(),
                    name: "missing".to_string(),
                    icon: None,
                    version: None,
                    detail: String::new(),
                    status: ToolStatus::Missing,
                    removable: true,
                },
            ],
        );

        assert_eq!(imported, 1);
        assert!(manifest.contains("npm", "installed"));
        assert!(!manifest.contains("npm", "missing"));
    }

    #[test]
    fn unmanaged_installed_packages_can_be_adopted() {
        assert_eq!(
            package_operations(ToolStatus::Installed, false, true),
            [ToolOperationKind::Adopt, ToolOperationKind::Uninstall]
        );
        assert_eq!(
            package_operations(ToolStatus::Outdated, true, true),
            [ToolOperationKind::Update, ToolOperationKind::Uninstall]
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
