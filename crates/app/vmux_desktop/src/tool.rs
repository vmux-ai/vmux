use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{BinReceive, UiEventPlugin};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use vmux_core::host::{UiState, UiStatePlugin};
use vmux_core::page::PageManifest;
use vmux_core::profile::vault::{GeneratedRecoveryKey, VaultRecovery};
use vmux_core::tool::{
    ToolAction, ToolCategory, ToolItem, ToolOpenRequest, ToolOperationKey, ToolOperationNotice,
    ToolProvider, ToolRequest, ToolStatus, ToolsNavigateRequest, ToolsRefreshRequest,
    ToolsSnapshot, ToolsUiState,
};
use vmux_core::vault::{
    VaultAction, VaultAuthorization, VaultCompletion, VaultOperation, VaultOperationState,
    VaultRefreshRequest, VaultRepository, VaultRequest, VaultSnapshot, VaultUiState,
};
use vmux_tool::{
    ExternalToolAction, ToolActionCompletion, ToolActionRequest, ToolStore, ToolStoreAction,
    ToolStoreTarget, ToolsManifest,
};

pub struct ToolPlugin;

impl Plugin for ToolPlugin {
    fn build(&self, app: &mut App) {
        let vault_root = vmux_core::profile::vault::root_dir();
        let _ = std::fs::create_dir_all(&vault_root);
        let knowledge_root = vault_root.join("knowledge");
        let tools_root = vault_root.join("tools");
        let _ = std::fs::create_dir_all(&knowledge_root);
        let _ = std::fs::create_dir_all(&tools_root);
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
                if watcher
                    .watch(&vault_root, RecursiveMode::NonRecursive)
                    .is_ok()
                    && watcher
                        .watch(&knowledge_root, RecursiveMode::Recursive)
                        .is_ok()
                    && watcher.watch(&tools_root, RecursiveMode::Recursive).is_ok()
                {
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
        app.world_mut().spawn((PAGE_MANIFEST, TOOLS_HOSTED_PAGE));
        app.world_mut().spawn((
            VAULT_PAGE_MANIFEST,
            vmux_core::host::page::NativelyHosted::page("vmux://vault/", "Vault"),
        ));
        app.world_mut().spawn((
            Name::new("Tool registry"),
            ToolRegistry::default(),
            ToolStore::current(),
            ToolsManifest::default(),
        ));
        app.init_resource::<ActionRequestSequence>()
            .init_resource::<VaultAutoSync>()
            .init_resource::<VaultRecoveryState>()
            .add_plugins((
                vmux_app::extension::McpConnectionPlugin,
                vmux_tool::ToolPlugin,
                UiStatePlugin::<ToolsUiState>::default(),
                UiStatePlugin::<VaultUiState>::default(),
            ))
            .add_plugins(UiEventPlugin::<(
                ToolsRefreshRequest,
                ToolRequest,
                ToolOpenRequest,
                ToolsNavigateRequest,
                VaultRequest,
                VaultRefreshRequest,
            )>::default())
            .add_observer(on_refresh_request)
            .add_observer(on_action_request)
            .add_observer(on_navigate_request)
            .add_observer(on_vault_action_request)
            .add_observer(on_vault_refresh_request)
            .add_observer(on_open_request)
            .add_systems(
                Update,
                (
                    drain_vault_watch,
                    start_tools_scan,
                    drain_tools_scan,
                    queue_vault_auto_sync,
                    start_tool_action,
                    start_external_tool_action,
                    drain_tool_actions,
                    start_vault_action,
                    drain_vault_actions,
                    emit_tools_state,
                    emit_vault_state,
                )
                    .chain(),
            )
            .add_systems(Update, drain_tool_store_actions.before(emit_tools_state));
    }
}

const PAGE_MANIFEST: PageManifest = PageManifest {
    host: "tools",
    title: "Tools",
    title_message_id: Some("tools-title"),
    replaces_command: None,
    keywords: &[
        "packages", "tools", "dotfiles", "homebrew", "npm", "mcp", "import",
    ],
    icon: Some(vmux_core::BuiltinIcon::Hammer),
    command_bar: true,
};

const TOOLS_HOSTED_PAGE: vmux_core::host::page::NativelyHosted =
    vmux_core::host::page::NativelyHosted::subtree("vmux://tools/", "Tools");

const VAULT_PAGE_MANIFEST: PageManifest = PageManifest {
    host: "vault",
    title: "Vault",
    title_message_id: Some("vault-title"),
    replaces_command: None,
    keywords: &["vault", "sync", "git", "backup", "dotfiles", "knowledge"],
    icon: Some(vmux_core::BuiltinIcon::Vault),
    command_bar: true,
};

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
    fn pending(operation_id: u64, request: &ToolRequest) -> Self {
        let mut subscriber = Self::default();
        subscriber.begin(operation_id, request);
        subscriber
    }

    fn begin(&mut self, operation_id: u64, request: &ToolRequest) {
        self.pending.insert(
            operation_id,
            ToolOperationKey::new(request.provider, request.action, request.id.clone()),
        );
        self.state.pending = self.pending.values().cloned().collect();
        self.state.notice = None;
        self.touch();
    }

    fn complete(
        &mut self,
        operation_id: u64,
        request: &ToolRequest,
        success: bool,
        message: String,
    ) {
        let operation = self.pending.remove(&operation_id).unwrap_or_else(|| {
            ToolOperationKey::new(request.provider, request.action, request.id.clone())
        });
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
#[require(UiState<VaultUiState>)]
struct VaultSubscriber {
    snapshot_revision: u64,
    revision: u64,
    emitted_revision: u64,
    state: VaultUiState,
}

impl VaultSubscriber {
    fn pending(operation_id: u64, action: VaultAction) -> Self {
        let mut subscriber = Self::default();
        subscriber.begin(operation_id, action);
        subscriber
    }

    fn begin(&mut self, operation_id: u64, action: VaultAction) {
        match action {
            VaultAction::ConnectCloud => self.state.cloud_root.clear(),
            VaultAction::GenerateRecoveryKey => self.state.generated_recovery_key.clear(),
            _ => {}
        }
        self.state.operation = Some(VaultOperation::pending(operation_id, action));
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

    fn complete(&mut self, operation_id: u64, completion: VaultCompletion) {
        let Some(operation) = self.state.operation.as_mut() else {
            return;
        };
        if operation.operation_id != operation_id {
            return;
        }
        if completion.success {
            match operation.action {
                VaultAction::GenerateRecoveryKey => {
                    self.state.generated_recovery_key = completion.message.clone();
                }
                VaultAction::CreateRecoveryKey => {
                    self.state.generated_recovery_key.clear();
                    self.state.recovery_upload_pending = completion.pending_upload;
                }
                VaultAction::Sync => {
                    self.state.recovery_upload_pending = false;
                }
                VaultAction::ConnectCloud => {
                    self.state.cloud_root = completion.message.clone();
                }
                _ => {}
            }
        }
        operation.state = VaultOperationState::Completed(completion);
        self.touch();
    }

    fn synchronize(&mut self, revision: u64, snapshot: &VaultSnapshot) {
        if self.snapshot_revision == revision {
            return;
        }
        self.snapshot_revision = revision;
        self.state.vault = snapshot.clone();
        self.touch();
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
struct ToolActionTask {
    task: Task<Result<String, String>>,
}

#[derive(Resource, Default)]
struct ActionRequestSequence(u64);

impl ActionRequestSequence {
    fn next(&mut self) -> u64 {
        let order = self.0;
        self.0 = self.0.wrapping_add(1);
        order
    }
}

#[derive(Component)]
struct PendingToolAction {
    order: u64,
    target: Entity,
    request: ToolRequest,
}

#[derive(Component)]
struct ToolOperationId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VaultActionTarget {
    Webview(Entity),
    Automatic,
}

impl VaultActionTarget {
    fn webview(self) -> Option<Entity> {
        match self {
            Self::Webview(entity) => Some(entity),
            Self::Automatic => None,
        }
    }
}

#[derive(Component)]
struct PendingVaultAction {
    order: u64,
    target: VaultActionTarget,
    request: VaultRequest,
}

#[derive(Component)]
struct VaultActionTask {
    operation_id: u64,
    target: VaultActionTarget,
    request: VaultRequest,
    task: Task<Result<VaultActionOutput, String>>,
    progress: Mutex<mpsc::Receiver<VaultAuthorization>>,
    canceled: Arc<AtomicBool>,
}

struct VaultActionOutput {
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

#[derive(Resource, Default)]
struct VaultAutoSync {
    requested: bool,
    initial_scan_complete: bool,
    remote_check: bool,
}

#[derive(Resource)]
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
    fn begin(&mut self, action: VaultAction) -> (VaultRecovery, Option<GeneratedRecoveryKey>) {
        let key = if action == VaultAction::CreateRecoveryKey {
            self.pending_key.take()
        } else {
            None
        };
        (self.service.clone(), key)
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

fn on_open_request(
    trigger: On<BinReceive<ToolOpenRequest>>,
    stores: Query<&ToolStore, With<ToolRegistry>>,
    mut requests: MessageWriter<vmux_layout::stack::StackRequest>,
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
    requests.write(vmux_layout::stack::StackRequest::Open {
        url: Some(url.to_string()),
    });
}

fn on_navigate_request(
    trigger: On<BinReceive<ToolsNavigateRequest>>,
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
    trigger: On<BinReceive<ToolsRefreshRequest>>,
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

fn on_action_request(
    trigger: On<BinReceive<ToolRequest>>,
    mut sequence: ResMut<ActionRequestSequence>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = trigger.event().payload.clone();
    let operation_id = sequence.next();
    if let Ok(mut subscriber) = subscribers.get_mut(target) {
        subscriber.begin(operation_id, &request);
    } else {
        commands
            .entity(target)
            .insert(ToolSubscriber::pending(operation_id, &request));
    }
    commands.spawn(PendingToolAction {
        order: operation_id,
        target,
        request,
    });
}

fn on_vault_action_request(
    trigger: On<BinReceive<VaultRequest>>,
    mut sequence: ResMut<ActionRequestSequence>,
    pending: Query<(Entity, &PendingVaultAction)>,
    tasks: Query<&VaultActionTask>,
    mut subscribers: Query<&mut VaultSubscriber>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = trigger.event().payload.clone();
    let operation_id = sequence.next();
    if let Ok(mut subscriber) = subscribers.get_mut(target) {
        subscriber.begin(operation_id, request.action);
    } else {
        commands
            .entity(target)
            .insert(VaultSubscriber::pending(operation_id, request.action));
    }
    let mut connecting = false;
    for task in &tasks {
        if task.request.action != VaultAction::ConnectGithub {
            continue;
        }
        connecting = true;
        task.canceled.store(true, Ordering::Relaxed);
    }
    if connecting {
        for (entity, _) in &pending {
            commands.entity(entity).despawn();
        }
    } else if request.action == VaultAction::ConnectGithub {
        for (entity, pending) in &pending {
            if pending.request.action == VaultAction::ConnectGithub {
                commands.entity(entity).despawn();
            }
        }
    }
    commands.spawn(PendingVaultAction {
        order: operation_id,
        target: VaultActionTarget::Webview(target),
        request,
    });
}

fn on_vault_refresh_request(
    trigger: On<BinReceive<VaultRefreshRequest>>,
    mut registry: Query<&mut ToolRegistry>,
    subscribers: Query<(), With<VaultSubscriber>>,
    mut commands: Commands,
) {
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    if !subscribers.contains(trigger.event().webview) {
        commands
            .entity(trigger.event().webview)
            .insert(VaultSubscriber::default());
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

fn drain_vault_watch(
    watcher: Option<NonSendMut<VaultWatch>>,
    mut auto_sync: ResMut<VaultAutoSync>,
    mut registry: Query<&mut ToolRegistry>,
) {
    let Some(watcher) = watcher else {
        return;
    };
    let Ok(mut state) = registry.single_mut() else {
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
    mut auto_sync: ResMut<VaultAutoSync>,
    registry: Query<&ToolRegistry>,
    scans: Query<(), With<ToolsScanTask>>,
    tasks: Query<&VaultActionTask>,
    pending: Query<&PendingVaultAction>,
    mut sequence: ResMut<ActionRequestSequence>,
    mut commands: Commands,
) {
    let Ok(state) = registry.single() else {
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
    if tasks
        .iter()
        .any(|task| task.request.action == VaultAction::Sync)
        || pending
            .iter()
            .any(|request| request.request.action == VaultAction::Sync)
    {
        auto_sync.requested = false;
        auto_sync.remote_check = false;
        return;
    }
    commands.spawn(PendingVaultAction {
        order: sequence.next(),
        target: VaultActionTarget::Automatic,
        request: VaultRequest {
            action: VaultAction::Sync,
            repository: String::new(),
            private: true,
            folder_name: String::new(),
            recovery_key: String::new(),
        },
    });
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

fn start_tool_action(
    pending: Query<(Entity, &PendingToolAction)>,
    requests: Query<(), With<ToolActionRequest>>,
    tasks: Query<(), With<ToolActionTask>>,
    store_actions: Query<(), With<ToolStoreAction>>,
    vault_tasks: Query<(), With<VaultActionTask>>,
    scans: Query<(), With<ToolsScanTask>>,
    stores: Query<Entity, (With<ToolStore>, With<ToolRegistry>)>,
    mut commands: Commands,
) {
    if !requests.is_empty()
        || !tasks.is_empty()
        || !store_actions.is_empty()
        || !vault_tasks.is_empty()
        || !scans.is_empty()
    {
        return;
    }
    let mut next = None;
    for (entity, request) in &pending {
        match next {
            Some((_, order)) if order <= request.order => {}
            _ => next = Some((entity, request.order)),
        }
    }
    let Some((entity, _)) = next else {
        return;
    };
    let Ok((_, pending_action)) = pending.get(entity) else {
        return;
    };
    let Ok(store) = stores.single() else {
        return;
    };
    commands
        .entity(entity)
        .remove::<PendingToolAction>()
        .insert((
            ToolOperationId(pending_action.order),
            ToolActionRequest::new(pending_action.target, pending_action.request.clone()),
            ToolStoreTarget::new(store),
        ));
}

fn start_external_tool_action(
    actions: Query<(Entity, &ToolActionRequest, &ToolStoreTarget), Added<ExternalToolAction>>,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, action, store) in &actions {
        let Ok(store) = stores.get(store.entity()) else {
            continue;
        };
        let request = action.request().clone();
        let task_request = request.clone();
        let store = store.clone();
        let task = IoTaskPool::get().spawn(async move { perform_action(&store, &task_request) });
        commands
            .entity(entity)
            .remove::<ExternalToolAction>()
            .insert(ToolActionTask { task });
    }
}

fn start_vault_action(
    pending: Query<(Entity, &PendingVaultAction)>,
    mut recovery: ResMut<VaultRecoveryState>,
    tasks: Query<(), With<VaultActionTask>>,
    tool_requests: Query<(), With<ToolActionRequest>>,
    tool_tasks: Query<(), With<ToolActionTask>>,
    store_actions: Query<(), With<ToolStoreAction>>,
    scans: Query<(), With<ToolsScanTask>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !tasks.is_empty()
        || !tool_requests.is_empty()
        || !tool_tasks.is_empty()
        || !store_actions.is_empty()
        || !scans.is_empty()
    {
        return;
    }
    let mut next = None;
    for (entity, request) in &pending {
        match next {
            Some((_, order)) if order <= request.order => {}
            _ => next = Some((entity, request.order)),
        }
    }
    let Some((entity, _)) = next else {
        return;
    };
    let Ok((_, pending_action)) = pending.get(entity) else {
        return;
    };
    let target = pending_action.target;
    let request = pending_action.request.clone();
    let (recovery, generated_recovery_key) = recovery.begin(request.action);
    let task_request = request.clone();
    let completion_wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let progress_wake = completion_wake.clone();
    let (progress_sender, progress_receiver) = mpsc::channel();
    let canceled = Arc::new(AtomicBool::new(false));
    let task_canceled = canceled.clone();
    let task = IoTaskPool::get().spawn(async move {
        let result = perform_vault_action(
            &task_request,
            recovery,
            generated_recovery_key,
            move |progress| {
                if progress_sender.send(progress).is_ok()
                    && let Some(wake) = &progress_wake
                {
                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                }
            },
            move || task_canceled.load(Ordering::Relaxed),
        )
        .await;
        if let Some(wake) = completion_wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
        result
    });
    commands
        .entity(entity)
        .remove::<PendingVaultAction>()
        .insert(VaultActionTask {
            operation_id: pending_action.order,
            target,
            request,
            task,
            progress: Mutex::new(progress_receiver),
            canceled,
        });
}

fn start_tools_scan(
    mut registry: Query<(&mut ToolRegistry, &ToolStore)>,
    tasks: Query<(), With<ToolsScanTask>>,
    action_tasks: Query<(), With<ToolActionTask>>,
    action_requests: Query<(), With<ToolActionRequest>>,
    store_actions: Query<(), With<ToolStoreAction>>,
    vault_tasks: Query<(), With<VaultActionTask>>,
    pending_actions: Query<(), With<PendingToolAction>>,
    pending_vault_actions: Query<(), With<PendingVaultAction>>,
    mut commands: Commands,
) {
    let Ok((mut state, store)) = registry.single_mut() else {
        return;
    };
    if !state.dirty
        || !tasks.is_empty()
        || !action_tasks.is_empty()
        || !action_requests.is_empty()
        || !store_actions.is_empty()
        || !vault_tasks.is_empty()
        || !pending_actions.is_empty()
        || !pending_vault_actions.is_empty()
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
    mut registry: Query<(&mut ToolRegistry, &mut ToolsManifest)>,
    mut auto_sync: ResMut<VaultAutoSync>,
    mut commands: Commands,
) {
    let Ok((mut state, mut manifest)) = registry.single_mut() else {
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

fn drain_tool_actions(
    mut tasks: Query<(
        Entity,
        &ToolOperationId,
        &ToolActionRequest,
        &mut ToolActionTask,
    )>,
    mut registry: Query<&mut ToolRegistry>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut commands: Commands,
) {
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    for (entity, operation_id, action, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let (success, message) = match result {
            Ok(message) => (true, message),
            Err(message) => (false, message),
        };
        let request = action.request();
        if let Ok(mut subscriber) = subscribers.get_mut(action.target()) {
            subscriber.complete(operation_id.0, request, success, message);
        }
        if success {
            state.dirty = true;
            state.full_scan = true;
            state.generation = state.generation.wrapping_add(1);
        }
    }
}

fn drain_tool_store_actions(
    actions: Query<
        (
            Entity,
            &ToolOperationId,
            &ToolActionRequest,
            &ToolActionCompletion,
        ),
        With<ToolStoreAction>,
    >,
    mut registry: Query<&mut ToolRegistry>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut commands: Commands,
) {
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    for (entity, operation_id, action, completion) in &actions {
        let request = action.request();
        if let Ok(mut subscriber) = subscribers.get_mut(action.target()) {
            subscriber.complete(
                operation_id.0,
                request,
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

fn drain_vault_actions(
    mut tasks: Query<(Entity, &mut VaultActionTask)>,
    mut registry: Query<&mut ToolRegistry>,
    mut recovery: ResMut<VaultRecoveryState>,
    mut subscribers: Query<&mut VaultSubscriber>,
    mut stack_requests: MessageWriter<vmux_layout::stack::StackRequest>,
    mut commands: Commands,
) {
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    for (entity, mut task) in &mut tasks {
        let target = task.target.webview();
        while let Ok(progress) = task.progress.get_mut().try_recv() {
            if let Some(target) = target {
                stack_requests.write(vmux_layout::stack::StackRequest::Open {
                    url: Some(progress.url.clone()),
                });
                if let Ok(mut subscriber) = subscribers.get_mut(target) {
                    subscriber.authorize(task.operation_id, progress);
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
        if task.request.action == VaultAction::Sync {
            state.snapshot.vault.sync_failed = !success;
            state.revision = state.revision.wrapping_add(1);
            if task.target == VaultActionTarget::Automatic && !success {
                continue;
            }
        }
        if let Some(target) = target
            && let Ok(mut subscriber) = subscribers.get_mut(target)
        {
            subscriber.complete(task.operation_id, completion);
        }
        state.dirty = true;
        state.full_scan |= !state.snapshot.loaded;
        state.load_vault_repositories |= task.request.action == VaultAction::ConnectGithub;
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
        UiState::<ToolsUiState>::write(&mut commands, entity, &subscriber.state);
        subscriber.emitted_revision = subscriber.revision;
    }
}

fn emit_vault_state(
    registry: Query<&ToolRegistry>,
    mut subscribers: Query<(Entity, &mut VaultSubscriber)>,
    mut commands: Commands,
) {
    let Ok(state) = registry.single() else {
        return;
    };
    for (entity, mut subscriber) in &mut subscribers {
        subscriber.synchronize(state.revision, &state.snapshot.vault);
        if subscriber.emitted_revision == subscriber.revision {
            continue;
        }
        UiState::<VaultUiState>::write(&mut commands, entity, &subscriber.state);
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
                actions: package_actions(item.status, managed, item.removable),
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
                actions: vec![ToolAction::Install, ToolAction::Forget],
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

fn package_actions(status: ToolStatus, managed: bool, removable: bool) -> Vec<ToolAction> {
    let mut actions = Vec::new();
    if !managed && matches!(status, ToolStatus::Installed | ToolStatus::Outdated) {
        actions.push(ToolAction::Adopt);
    }
    if status == ToolStatus::Outdated {
        actions.push(ToolAction::Update);
    }
    if status == ToolStatus::Missing {
        actions.push(ToolAction::Install);
    }
    if removable {
        actions.push(ToolAction::Uninstall);
    }
    actions
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
    let (discovered, discovery_errors) = store.discover_mcp_servers();
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
            let actions = if managed {
                vec![ToolAction::Forget]
            } else if status == ToolStatus::Available {
                vec![ToolAction::Adopt]
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
                actions,
            }
        })
        .collect();
    ToolCategory {
        provider: ToolProvider::Mcp,
        items,
    }
}

fn scan_dotfiles(store: &ToolStore, manifest: &mut ToolsManifest) -> ToolCategory {
    let discovered = store.dotfile_packages().unwrap_or_default();
    for package in &discovered {
        manifest.set_dotfile_package(package, true);
    }
    let mut package_names = discovered.into_iter().collect::<BTreeSet<_>>();
    package_names.extend(manifest.dotfiles.packages.iter().cloned());
    let mut items = Vec::new();
    for package in package_names {
        let managed = manifest.dotfiles.packages.contains(&package);
        let (status, detail, actions) = match store.plan_dotfile_package(&package) {
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
                let actions = if managed {
                    vec![ToolAction::Link, ToolAction::Unlink]
                } else {
                    vec![ToolAction::Link]
                };
                (status, detail, actions)
            }
            Err(error) => (
                ToolStatus::Missing,
                error,
                if managed {
                    vec![ToolAction::Unlink]
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
            actions,
        });
    }
    ToolCategory {
        provider: ToolProvider::Dotfiles,
        items,
    }
}

fn perform_action(store: &ToolStore, request: &ToolRequest) -> Result<String, String> {
    if request.action == ToolAction::Apply {
        return apply_manifest(store);
    }
    if request.action == ToolAction::Import {
        return import_provider(store, request.provider, request.value.trim());
    }
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    match request.action {
        ToolAction::Install => {
            set_manifest_entry(store, request.provider, &request.id, true)?;
            install_provider(store, request.provider, &request.id)?;
            Ok(format!("{} installed", request.id))
        }
        ToolAction::Update => {
            set_manifest_entry(store, request.provider, &request.id, true)?;
            update_provider(store, request.provider, &request.id)?;
            Ok(format!("{} updated", request.id))
        }
        ToolAction::Uninstall => {
            uninstall_provider(store, request.provider, &request.id)?;
            set_manifest_entry(store, request.provider, &request.id, false)?;
            Ok(format!("{} removed", request.id))
        }
        ToolAction::Forget => {
            set_manifest_entry(store, request.provider, &request.id, false)?;
            Ok(format!("{} removed from tools.toml", request.id))
        }
        ToolAction::Adopt => {
            if request.provider == ToolProvider::Dotfiles {
                if request.value.trim().is_empty() {
                    return Err("dotfile path is required".to_string());
                }
                let destination =
                    store.adopt_dotfile(Path::new(request.value.trim()), request.id.trim())?;
                Ok(format!("adopted {}", destination.display()))
            } else if request.provider == ToolProvider::Mcp {
                store.import_discovered_mcp_server(&request.id)?;
                Ok(format!("{} is now managed", request.id))
            } else {
                set_manifest_entry(store, request.provider, &request.id, true)?;
                Ok(format!("{} is now managed", request.id))
            }
        }
        ToolAction::Link => {
            if request.provider != ToolProvider::Dotfiles {
                return Err("link is only valid for dotfiles".to_string());
            }
            set_manifest_entry(store, request.provider, &request.id, true)?;
            let linked = store.apply_dotfile_package(&request.id)?;
            Ok(format!("linked {linked} file(s)"))
        }
        ToolAction::Unlink => {
            if request.provider != ToolProvider::Dotfiles {
                return Err("unlink is only valid for dotfiles".to_string());
            }
            let removed = store.disable_and_unlink_dotfile_package(&request.id)?;
            Ok(format!("unlinked {removed} file(s)"))
        }
        ToolAction::Apply | ToolAction::Import => unreachable!(),
    }
}

async fn perform_vault_action<F, C>(
    request: &VaultRequest,
    recovery: VaultRecovery,
    generated_recovery_key: Option<GeneratedRecoveryKey>,
    progress: F,
    canceled: C,
) -> Result<VaultActionOutput, String>
where
    F: Fn(VaultAuthorization),
    C: Fn() -> bool,
{
    if request.action == VaultAction::CreateRecoveryKey {
        let key = generated_recovery_key
            .ok_or_else(|| "No Recovery Key has been generated for this Vault".to_string())?;
        let result = recovery.create(key)?;
        return Ok(VaultActionOutput {
            message: String::new(),
            pending_upload: result.pending_upload,
            generated_recovery_key: None,
        });
    }
    let message = match request.action {
        VaultAction::Create => vmux_core::profile::vault::create_remote(
            &request.repository,
            if request.private {
                vmux_core::profile::vault::RepositoryVisibility::Private
            } else {
                vmux_core::profile::vault::RepositoryVisibility::Public
            },
        ),
        VaultAction::Connect => vmux_core::profile::vault::connect_remote(&request.repository),
        VaultAction::Sync => vmux_core::profile::vault::sync(),
        VaultAction::ConnectGithub => vmux_core::profile::vault::connect_github_with_progress(
            |code| {
                progress(VaultAuthorization {
                    code,
                    url: "https://github.com/login/device".to_string(),
                });
            },
            canceled,
        ),
        VaultAction::ConnectFolder => {
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
            vmux_core::profile::vault::connect_folder(folder.path())
        }
        VaultAction::GenerateRecoveryKey => {
            let key = GeneratedRecoveryKey::generate()?;
            let message = key.display().to_string();
            return Ok(VaultActionOutput {
                message,
                pending_upload: false,
                generated_recovery_key: Some(key),
            });
        }
        VaultAction::CreateRecoveryKey => unreachable!(),
        VaultAction::UnlockRecoveryKey => recovery.unlock(&request.recovery_key),
        VaultAction::ConnectCloud => connect_cloud_storage(&request.repository).await,
        VaultAction::CreateCloudFolder => {
            let folder = Path::new(&request.repository).join(&request.folder_name);
            vmux_core::profile::vault::connect_folder(&folder)
        }
        VaultAction::ChooseCloudFolder => {
            let mut dialog = rfd::AsyncFileDialog::new();
            let root = Path::new(&request.repository);
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
            vmux_core::profile::vault::connect_folder(folder.path())
        }
    }?;
    Ok(VaultActionOutput {
        message,
        pending_upload: false,
        generated_recovery_key: None,
    })
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
                let (formulae, casks) = store.import_brewfile(Path::new(path))?;
                Ok(format!("imported {formulae} formulae and {casks} casks"))
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
                let imported = store.import_npm_manifest(Path::new(path))?;
                Ok(format!("imported {imported} NPM package(s)"))
            } else {
                import_scanned_inventory(store, provider, scan_npm(false)?)
            }
        }
        ToolProvider::Acp => import_scanned_inventory(store, provider, scan_acp(false)?),
        ToolProvider::Lsp => import_scanned_inventory(store, provider, scan_lsp(false)?),
        ToolProvider::Mcp => {
            let imported = if path.is_empty() {
                store.import_default_mcp_configs()?
            } else {
                store.import_mcp_config(Path::new(path))?
            };
            Ok(format!("imported {imported} MCP server(s)"))
        }
        ToolProvider::Dotfiles => {
            if path.is_empty() {
                let packages = store.dotfile_packages()?;
                let mut manifest = store.load()?;
                let mut imported = 0;
                for package in packages {
                    imported += usize::from(!manifest.dotfiles.packages.contains(&package));
                    manifest.set_dotfile_package(&package, true);
                }
                store.save(&manifest)?;
                Ok(format!("imported {imported} dotfile package(s)"))
            } else {
                let imported = store.import_dotfiles(Path::new(path))?;
                Ok(format!("imported {imported} dotfile package(s)"))
            }
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
    let linked = store.apply_enabled_dotfiles(&manifest)?;
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
            store.apply_dotfile_package(id)?;
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
            store.unlink_dotfile_package(id)?;
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
        fn pending_targets(vault: VaultSnapshot, remote_check: bool) -> Vec<VaultActionTarget> {
            let mut app = App::new();
            app.init_resource::<VaultAutoSync>()
                .init_resource::<ActionRequestSequence>()
                .add_systems(Update, queue_vault_auto_sync);
            let registry = app.world_mut().spawn(ToolRegistry::default()).id();
            {
                let mut state = app.world_mut().get_mut::<ToolRegistry>(registry).unwrap();
                state.dirty = false;
                state.snapshot.loaded = true;
                state.snapshot.vault = vault;
            }
            let mut auto_sync = app.world_mut().resource_mut::<VaultAutoSync>();
            auto_sync.requested = true;
            auto_sync.remote_check = remote_check;

            app.update();

            let world = app.world_mut();
            let mut query = world.query::<&PendingVaultAction>();
            query.iter(world).map(|pending| pending.target).collect()
        }
    }

    #[test]
    fn tools_page_owns_provider_routes() {
        assert!(TOOLS_HOSTED_PAGE.answers_for("vmux://tools/extensions"));
        assert!(TOOLS_HOSTED_PAGE.answers_for("vmux://tools/homebrew"));
        assert!(!TOOLS_HOSTED_PAGE.answers_for("vmux://toolbox/"));
    }

    #[test]
    fn recovery_key_is_retained_until_one_create_attempt() {
        let mut recovery = VaultRecoveryState::default();
        recovery.retain(GeneratedRecoveryKey::generate().unwrap());

        assert!(recovery.begin(VaultAction::Sync).1.is_none());
        assert!(recovery.begin(VaultAction::CreateRecoveryKey).1.is_some());
        assert!(recovery.begin(VaultAction::CreateRecoveryKey).1.is_none());
    }

    #[test]
    fn tool_operation_state_tracks_pending_and_completion() {
        let request = ToolRequest {
            provider: ToolProvider::Npm,
            action: ToolAction::Install,
            id: "typescript".to_string(),
            value: String::new(),
        };
        let mut subscriber = ToolSubscriber::pending(7, &request);

        assert_eq!(
            subscriber.state.pending,
            vec![ToolOperationKey::new(
                ToolProvider::Npm,
                ToolAction::Install,
                "typescript",
            )]
        );

        subscriber.complete(7, &request, true, "installed".to_string());

        assert!(subscriber.state.pending.is_empty());
        let notice = subscriber.state.notice.as_ref().unwrap();
        assert!(notice.success);
        assert_eq!(notice.message, "installed");
    }

    #[test]
    fn vault_operation_state_preserves_recovery_workflow() {
        let mut subscriber = VaultSubscriber::pending(3, VaultAction::GenerateRecoveryKey);
        subscriber.complete(
            3,
            VaultCompletion {
                success: true,
                message: "recovery-key".to_string(),
                pending_upload: false,
            },
        );
        assert_eq!(subscriber.state.generated_recovery_key, "recovery-key");

        subscriber.begin(4, VaultAction::CreateRecoveryKey);
        subscriber.complete(
            4,
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
    fn tools_tab_navigation_replaces_the_owning_stack() {
        let mut app = App::new();
        app.add_message::<vmux_core::PageOpenRequest>()
            .add_observer(on_navigate_request);
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::Stack::default())
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive {
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
            category.items[0].actions,
            [ToolAction::Install, ToolAction::Forget]
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
            package_actions(ToolStatus::Installed, false, true),
            [ToolAction::Adopt, ToolAction::Uninstall]
        );
        assert_eq!(
            package_actions(ToolStatus::Outdated, true, true),
            [ToolAction::Update, ToolAction::Uninstall]
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
    fn automatic_backup_creates_only_needed_pending_actions() {
        let connected = VaultSnapshot {
            initialized: true,
            unlocked: true,
            remote: "https://example.com/vault.git".to_string(),
            dirty: 1,
            ..Default::default()
        };
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(connected.clone(), false),
            [VaultActionTarget::Automatic]
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
            [VaultActionTarget::Automatic]
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
            [VaultActionTarget::Automatic]
        );
    }
}
