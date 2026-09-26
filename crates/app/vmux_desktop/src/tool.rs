use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Output};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_core::host::{UiState, UiStatePlugin, UiStateWrite};
use vmux_core::tool::{
    ToolAdoptRequest, ToolApplyRequest, ToolCategory, ToolForgetRequest, ToolImportRequest,
    ToolInstallRequest, ToolItem, ToolLinkRequest, ToolOpenRequest, ToolOperationKey,
    ToolOperationKind, ToolOperationNotice, ToolProvider, ToolStatus, ToolUninstallRequest,
    ToolUnlinkRequest, ToolUpdateRequest, ToolsNavigateRequest, ToolsRefreshRequest, ToolsSnapshot,
    ToolsUiState,
};
use vmux_tool::{
    ExternalToolOperation, ToolOperationFailed, ToolOperationRequest, ToolOperationSucceeded,
    ToolStore, ToolStoreOperation, ToolStoreTarget, ToolsManifest,
};

pub(crate) struct ToolUiPlugin;

impl Plugin for ToolUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_tool::McpConnectionPlugin,
            UiStatePlugin::<ToolsUiState>::default(),
        ))
        .add_plugins(UiEventPlugin::<(
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
        )>::default())
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
        .add_observer(on_open_request)
        .add_systems(Startup, spawn_tool_registry)
        .add_systems(
            Update,
            (
                start_tools_scan,
                drain_tools_scan,
                start_tool_operation,
                drain_tool_operations,
                emit_tools_state,
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
                drain_succeeded_tool_store_operations,
                drain_failed_tool_store_operations,
            )
                .before(emit_tools_state),
        );
    }
}

fn spawn_tool_registry(mut commands: Commands) {
    commands.spawn((
        Name::new("Tool registry"),
        ToolRegistry::default(),
        ToolStore::current(),
        ToolsManifest::default(),
        OperationRequestSequence::default(),
    ));
}

#[derive(Component)]
struct ToolRegistry {
    dirty: bool,
    refresh_catalogs: bool,
    generation: u64,
    revision: u64,
    snapshot: ToolsSnapshot,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            dirty: true,
            refresh_catalogs: false,
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
        ToolOperationRequest(request),
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

fn start_tool_operation(
    pending: Query<(Entity, &ToolOperationContext), With<PendingToolOperation>>,
    active: Query<(), With<ToolStoreTarget>>,
    scans: Query<(), With<ToolsScanTask>>,
    stores: Query<Entity, (With<ToolStore>, With<ToolRegistry>)>,
    mut commands: Commands,
) {
    if !active.is_empty() || !scans.is_empty() {
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
        .insert(ToolStoreTarget(store));
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
                let Ok(store) = stores.get(target.0).cloned() else {
                    continue;
                };
                let request = operation.0.clone();
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

fn start_tools_scan(
    mut registry: Query<(&mut ToolRegistry, &ToolStore)>,
    tasks: Query<(), With<ToolsScanTask>>,
    tool_operations: Query<(), With<ToolStoreTarget>>,
    pending_tool_operations: Query<(), With<PendingToolOperation>>,
    mut commands: Commands,
) {
    let Ok((mut state, store)) = registry.single_mut() else {
        return;
    };
    if !state.dirty
        || !tasks.is_empty()
        || !tool_operations.is_empty()
        || !pending_tool_operations.is_empty()
    {
        return;
    }
    let generation = state.generation;
    let refresh_catalogs = state.refresh_catalogs;
    let store = store.clone();
    state.dirty = false;
    state.refresh_catalogs = false;
    let task = IoTaskPool::get().spawn(async move { scan_tools(&store, refresh_catalogs) });
    commands.spawn(ToolsScanTask { generation, task });
}

fn drain_tools_scan(
    mut tasks: Query<(Entity, &mut ToolsScanTask)>,
    mut registry: Query<(&mut ToolRegistry, &mut ToolsManifest)>,
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
            state.generation = state.generation.wrapping_add(1);
        }
    }
}

fn drain_succeeded_tool_store_operations(
    operations: Query<
        (Entity, &ToolOperationContext, &ToolOperationSucceeded),
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
                true,
                completion.0.clone(),
            );
        }
        state.dirty = true;
        state.generation = state.generation.wrapping_add(1);
        commands.entity(entity).despawn();
    }
}

fn drain_failed_tool_store_operations(
    operations: Query<(Entity, &ToolOperationContext, &ToolOperationFailed)>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut commands: Commands,
) {
    for (entity, operation, failure) in &operations {
        if let Ok(mut subscriber) = subscribers.get_mut(operation.target) {
            subscriber.complete(
                operation.operation_id,
                operation.operation.clone(),
                false,
                failure.0.clone(),
            );
        }
        commands.entity(entity).despawn();
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

fn scan_tools(store: &ToolStore, refresh_catalogs: bool) -> ToolsScanOutput {
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
            categories,
            installed,
            updates,
            conflicts,
            error: errors.join("\n"),
        },
        manifest,
    }
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
    let snapshot = scan_tools(store, false).snapshot;
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
}
