use std::collections::BTreeMap;
use std::path::Path;

use crate::state::{
    ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest, ToolInstallRequest,
    ToolLinkRequest, ToolOpenRequest, ToolOperationKey, ToolOperationKind, ToolOperationNotice,
    ToolProvider, ToolProviderMetadata, ToolStatus, ToolUninstallRequest, ToolUnlinkRequest,
    ToolUpdateRequest, ToolsFilterRequest, ToolsNavigateRequest, ToolsRefreshRequest,
    ToolsSnapshot, ToolsUiState, ToolsView,
};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::manifest::{FeatureId, FeatureManifest};
use vmux_ecs::page::PageReady;
use vmux_ecs::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_ecs::{UiState, UiStatePlugin, UiStateWrite};

use crate::{
    ExternalToolOperation, ToolApplier, ToolOperationFailed, ToolOperationFinished,
    ToolOperationRequest, ToolOperationSucceeded, ToolOperator, ToolProviderBinding,
    ToolProviderId, ToolProviderTarget, ToolScanner, ToolStore, ToolStoreTarget, ToolsManifest,
};

pub(crate) struct ToolHostPlugin;

impl Plugin for ToolHostPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::McpConnectionPlugin,
            UiStatePlugin::<ToolsUiState>::default(),
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
            )>::default(),
            UiEventPlugin::<(
                ToolUnlinkRequest,
                ToolApplyRequest,
                ToolImportRequest,
                ToolOpenRequest,
                ToolsFilterRequest,
                ToolsNavigateRequest,
            )>::default(),
        ))
        .add_observer(page_ready)
        .add_observer(refresh_request)
        .add_observer(operation_request::<ToolInstallRequest>)
        .add_observer(operation_request::<ToolUpdateRequest>)
        .add_observer(operation_request::<ToolUninstallRequest>)
        .add_observer(operation_request::<ToolForgetRequest>)
        .add_observer(operation_request::<ToolAdoptRequest>)
        .add_observer(operation_request::<ToolLinkRequest>)
        .add_observer(operation_request::<ToolUnlinkRequest>)
        .add_observer(operation_request::<ToolApplyRequest>)
        .add_observer(operation_request::<ToolImportRequest>)
        .add_observer(open_request)
        .add_observer(filter_request)
        .add_observer(navigate_request)
        .add_systems(Startup, spawn_registry)
        .add_systems(
            Update,
            (
                bind,
                ApplyDeferred,
                request_scan,
                finish_scans,
                start_operation,
                start_external_operations,
                finish_external_operations,
                drain_succeeded_operations,
                drain_failed_operations,
                emit_tools_state,
            )
                .chain(),
        );
    }
}

fn bind(
    providers: Query<(Entity, &ToolProviderBinding), Without<ToolProviderId>>,
    manifests: Query<(&FeatureId, &FeatureManifest)>,
    mut commands: Commands,
) {
    for (entity, binding) in &providers {
        let Some(manifest) = manifests
            .iter()
            .find_map(|(feature, manifest)| (*feature == binding.feature()).then_some(manifest))
        else {
            continue;
        };
        let Some(provider) = manifest.tool_providers.get(binding.index()) else {
            continue;
        };
        commands.entity(entity).insert((
            Name::new(format!("{} tool provider", provider.title)),
            ToolProviderId(ToolProvider::new(provider.id.clone())),
        ));
    }
}

fn page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<&PageMetadata>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(page) = pages.get(webview) else {
        return;
    };
    if !page.url.starts_with(crate::ToolPlugin::URL) {
        return;
    }
    if let Ok(mut subscriber) = subscribers.get_mut(webview) {
        subscriber.navigate(&page.url);
    } else {
        let mut subscriber = ToolSubscriber::default();
        subscriber.navigate(&page.url);
        commands.entity(webview).insert(subscriber);
    }
}

fn filter_request(
    trigger: On<UiInput<ToolsFilterRequest>>,
    mut subscribers: Query<&mut ToolSubscriber>,
) {
    let Ok(mut subscriber) = subscribers.get_mut(trigger.event().webview) else {
        return;
    };
    subscriber.filter(&trigger.event().payload.query);
}

fn open_request(
    trigger: On<UiInput<ToolOpenRequest>>,
    stores: Query<&ToolStore>,
    mut requests: MessageWriter<PageOpenRequest>,
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
    requests.write(PageOpenRequest {
        target: PageOpenTarget::NewStack,
        url: url.to_string(),
        request_id: None,
    });
}

fn navigate_request(
    trigger: On<UiInput<ToolsNavigateRequest>>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut requests: MessageWriter<PageOpenRequest>,
) {
    let Some(route) = crate::route::ToolRoute::parse(&trigger.event().payload.url) else {
        return;
    };
    if let Ok(mut subscriber) = subscribers.get_mut(trigger.event().webview) {
        subscriber.navigate(&route.url());
    }
    requests.write(PageOpenRequest {
        target: PageOpenTarget::ContainingStack(trigger.event().webview),
        url: route.url(),
        request_id: None,
    });
}

fn spawn_registry(mut commands: Commands) {
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
        self.project();
        self.touch();
    }

    fn navigate(&mut self, url: &str) {
        let Some(route) = crate::route::ToolRoute::parse(url) else {
            return;
        };
        if self.state.view.route == route.id() {
            return;
        }
        self.state.view.route = route.id().to_string();
        self.project();
        self.touch();
    }

    fn filter(&mut self, query: &str) {
        let query = query.to_string();
        if self.state.view.query == query {
            return;
        }
        self.state.view.query = query;
        self.project();
        self.touch();
    }

    fn project(&mut self) {
        let snapshot = &self.state.snapshot;
        let route = if self.state.view.route.is_empty() {
            snapshot
                .providers
                .first()
                .map(|provider| provider.route.clone())
                .unwrap_or_default()
        } else {
            self.state.view.route.clone()
        };
        let query = self.state.view.query.trim().to_ascii_lowercase();
        let provider = snapshot
            .providers
            .iter()
            .find(|provider| provider.route == route);
        let mut categories = Vec::new();
        for category in &snapshot.categories {
            let Some(metadata) = snapshot.provider(&category.provider) else {
                continue;
            };
            if metadata.route != route {
                continue;
            }
            let mut items = Vec::new();
            for item in &category.items {
                if query.is_empty()
                    || item.name.to_ascii_lowercase().contains(&query)
                    || item.id.to_ascii_lowercase().contains(&query)
                    || item.detail.to_ascii_lowercase().contains(&query)
                    || metadata.title.to_ascii_lowercase().contains(&query)
                {
                    items.push(item.clone());
                }
            }
            if !items.is_empty() {
                categories.push(crate::state::ToolCategory {
                    provider: category.provider.clone(),
                    items,
                });
            }
        }
        let visible_count = categories
            .iter()
            .map(|category| category.items.len() as u32)
            .sum();
        self.state.view = ToolsView {
            route: route.clone(),
            query: self.state.view.query.clone(),
            route_title: provider
                .map(|provider| provider.route_title.clone())
                .unwrap_or_default(),
            route_title_message_id: provider
                .map(|provider| provider.route_title_message_id.clone())
                .unwrap_or_default(),
            categories,
            visible_count,
            apply_provider: provider
                .filter(|provider| provider.apply)
                .map(|provider| provider.provider.clone()),
            show_brewfile: snapshot
                .providers
                .iter()
                .any(|provider| provider.route == route && provider.brewfile),
        };
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

#[derive(Component)]
struct ToolScanTask {
    generation: u64,
    task: Task<ToolScanResult>,
}

struct ToolScanResult {
    snapshot: ToolsSnapshot,
    manifest: ToolsManifest,
}

#[derive(Component)]
struct ToolProviderOperationTask(Task<Result<String, String>>);

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

fn refresh_request(
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

fn operation_request<R>(
    trigger: On<UiInput<R>>,
    registries: Query<&mut OperationRequestSequence, With<ToolRegistry>>,
    subscribers: Query<&mut ToolSubscriber>,
    commands: Commands,
) where
    R: Clone + Send + Sync + 'static,
    ToolOperationKey: From<R>,
{
    let request = trigger.event().payload.clone();
    let operation = ToolOperationKey::from(request.clone());
    queue_tool_operation(
        trigger.event().webview,
        request,
        operation,
        registries,
        subscribers,
        commands,
    );
}

fn start_operation(
    pending: Query<(Entity, &ToolOperationContext), With<PendingToolOperation>>,
    active: Query<(), With<ToolStoreTarget>>,
    scans: Query<(), With<ToolScanTask>>,
    stores: Query<Entity, (With<ToolStore>, With<ToolRegistry>)>,
    providers: Query<(Entity, &ToolProviderId)>,
    mut commands: Commands,
) {
    if !active.is_empty() || !scans.is_empty() {
        return;
    }
    let mut next = None;
    for (entity, operation) in &pending {
        match next {
            Some((_, operation_id, _)) if operation_id <= operation.operation_id => {}
            _ => {
                next = Some((
                    entity,
                    operation.operation_id,
                    operation.operation.provider.clone(),
                ))
            }
        }
    }
    let Some((entity, _, provider_id)) = next else {
        return;
    };
    let Ok(store) = stores.single() else {
        return;
    };
    let mut operation_entity = commands.entity(entity);
    operation_entity
        .remove::<PendingToolOperation>()
        .insert(ToolStoreTarget::new(store));
    if let Some((provider, _)) = providers
        .iter()
        .find(|(_, provider)| provider.0 == provider_id)
    {
        operation_entity.insert(ToolProviderTarget::new(provider));
    }
}

fn request_scan(
    mut registry: Query<(&mut ToolRegistry, &ToolStore)>,
    providers: Query<(&ToolProviderId, &ToolScanner)>,
    features: Query<&FeatureManifest>,
    scans: Query<(), With<ToolScanTask>>,
    tool_operations: Query<(), With<ToolStoreTarget>>,
    pending_tool_operations: Query<(), With<PendingToolOperation>>,
    mut commands: Commands,
) {
    let Ok((mut state, store)) = registry.single_mut() else {
        return;
    };
    if !state.dirty
        || !scans.is_empty()
        || !tool_operations.is_empty()
        || !pending_tool_operations.is_empty()
    {
        return;
    }
    let generation = state.generation;
    let refresh_catalogs = state.refresh_catalogs;
    let store = store.clone();
    let mut providers = providers
        .iter()
        .map(|(provider, scanner)| (provider.clone(), *scanner))
        .collect::<Vec<_>>();
    providers.sort_by(|(left, _), (right, _)| left.0.cmp(&right.0));
    let mut metadata = Vec::new();
    for feature in &features {
        for entry in &feature.tool_providers {
            metadata.push(ToolProviderMetadata {
                provider: ToolProvider::new(entry.id.clone()),
                title: entry.title.clone(),
                title_message_id: entry.title_message_id.clone(),
                route_title: entry.route_title.clone(),
                route_title_message_id: entry.route_title_message_id.clone(),
                short_label: entry.short_label.clone(),
                route: entry.route.clone(),
                rank: entry.rank,
                thumbnails: entry.thumbnails,
                apply: entry.apply,
                brewfile: entry.brewfile,
            });
        }
    }
    metadata.sort_by_key(|entry| entry.rank);
    metadata.dedup_by(|left, right| left.provider == right.provider);
    let task = IoTaskPool::get()
        .spawn(async move { scan_tools(&store, &providers, metadata, refresh_catalogs) });
    commands.spawn((
        Name::new("Tool inventory scan"),
        ToolScanTask { generation, task },
    ));
    state.dirty = false;
    state.refresh_catalogs = false;
}

fn finish_scans(
    mut scans: Query<(Entity, &mut ToolScanTask)>,
    mut registry: Query<(&mut ToolRegistry, &mut ToolsManifest)>,
    mut commands: Commands,
) {
    let Ok((mut state, mut manifest)) = registry.single_mut() else {
        return;
    };
    for (entity, mut scan) in &mut scans {
        let Some(output) = future::block_on(future::poll_once(&mut scan.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if scan.generation != state.generation {
            state.dirty = true;
            continue;
        }
        state.snapshot.clone_from(&output.snapshot);
        manifest.clone_from(&output.manifest);
        state.revision = state.revision.wrapping_add(1);
    }
}

fn start_external_operations(
    operations: Query<
        (
            Entity,
            &ToolOperationContext,
            &ToolStoreTarget,
            Option<&ToolOperationRequest<ToolAdoptRequest>>,
            Option<&ToolOperationRequest<ToolImportRequest>>,
        ),
        (
            Added<ExternalToolOperation>,
            Without<ToolProviderOperationTask>,
        ),
    >,
    providers: Query<(&ToolProviderId, &ToolOperator)>,
    appliers: Query<&ToolApplier>,
    registry: Query<&ToolRegistry>,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, context, target, adopt, import) in &operations {
        let value = match context.operation.kind {
            ToolOperationKind::Adopt => adopt
                .map(|request| request.0.value.clone())
                .unwrap_or_default(),
            ToolOperationKind::Import => import
                .map(|request| request.0.value.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let Ok(store) = stores.get(target.entity()).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        if context.operation.kind == ToolOperationKind::Apply {
            let Ok(registry) = registry.single() else {
                continue;
            };
            let operators = providers
                .iter()
                .map(|(provider, operator)| (provider.clone(), *operator))
                .collect::<Vec<_>>();
            let installs = registry
                .snapshot
                .categories
                .iter()
                .flat_map(|category| &category.items)
                .filter(|item| item.managed && item.status == ToolStatus::Missing)
                .filter(|item| {
                    operators
                        .iter()
                        .any(|(provider, _)| provider.0 == item.provider)
                })
                .map(|item| {
                    ToolOperationKey::new(
                        item.provider.clone(),
                        ToolOperationKind::Install,
                        item.id.clone(),
                    )
                })
                .collect::<Vec<_>>();
            let appliers = appliers.iter().copied().collect::<Vec<_>>();
            let task = IoTaskPool::get()
                .spawn(async move { apply_tools(&store, &installs, &operators, &appliers) });
            commands
                .entity(entity)
                .remove::<ExternalToolOperation>()
                .insert(ToolProviderOperationTask(task));
            continue;
        }
        let Some((_, operator)) = providers
            .iter()
            .find(|(provider, _)| provider.0 == context.operation.provider)
        else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed(format!(
                    "{} does not support this operation",
                    context.operation.provider.id()
                )),
            ));
            continue;
        };
        let operation = context.operation.clone();
        let operator = *operator;
        let task = IoTaskPool::get()
            .spawn(async move { operator.run(&store, &operation, value.as_str()) });
        commands
            .entity(entity)
            .remove::<ExternalToolOperation>()
            .insert(ToolProviderOperationTask(task));
    }
}

fn apply_tools(
    store: &ToolStore,
    installs: &[ToolOperationKey],
    operators: &[(ToolProviderId, ToolOperator)],
    appliers: &[ToolApplier],
) -> Result<String, String> {
    let mut installed = 0;
    for operation in installs {
        let Some((_, operator)) = operators
            .iter()
            .find(|(provider, _)| provider.0 == operation.provider)
        else {
            return Err(format!(
                "{} does not support install",
                operation.provider.id()
            ));
        };
        operator.run(store, operation, "")?;
        installed += 1;
    }
    let mut applied = 0;
    for applier in appliers {
        applied += applier.run(store)?;
    }
    Ok(format!(
        "installed {installed} package(s), linked {applied} file(s)"
    ))
}

fn finish_external_operations(
    mut operations: Query<(Entity, &mut ToolProviderOperationTask)>,
    mut commands: Commands,
) {
    for (entity, mut operation) in &mut operations {
        let Some(result) = future::block_on(future::poll_once(&mut operation.0)) else {
            continue;
        };
        let mut entity = commands.entity(entity);
        entity.remove::<ToolProviderOperationTask>();
        match result {
            Ok(message) => {
                entity.insert((ToolOperationFinished, ToolOperationSucceeded(message)));
            }
            Err(message) => {
                entity.insert((ToolOperationFinished, ToolOperationFailed(message)));
            }
        }
    }
}

fn scan_tools(
    store: &ToolStore,
    providers: &[(ToolProviderId, ToolScanner)],
    metadata: Vec<ToolProviderMetadata>,
    refresh_catalogs: bool,
) -> ToolScanResult {
    let (mut manifest, manifest_error) = match store.load() {
        Ok(manifest) => (manifest, None),
        Err(error) => (ToolsManifest::default(), Some(error)),
    };
    let can_persist = manifest_error.is_none();
    let original_manifest = manifest.clone();
    let mut categories = Vec::new();
    let mut errors = manifest_error.into_iter().collect::<Vec<_>>();
    for (provider, scanner) in providers {
        match scanner.scan(&provider.0, store, &mut manifest, refresh_catalogs) {
            Ok(snapshot) => {
                categories.push(snapshot.category);
                errors.extend(snapshot.errors);
            }
            Err(error) => errors.push(format!("{}: {error}", provider.0.id())),
        }
    }
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
    ToolScanResult {
        snapshot: ToolsSnapshot {
            loaded: true,
            root: store.root().to_string_lossy().into_owned(),
            categories,
            providers: metadata,
            installed,
            updates,
            conflicts,
            error: errors.join("\n"),
        },
        manifest,
    }
}

fn drain_succeeded_operations(
    operations: Query<
        (Entity, &ToolOperationContext, &ToolOperationSucceeded),
        With<ToolOperationFinished>,
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

fn drain_failed_operations(
    operations: Query<
        (Entity, &ToolOperationContext, &ToolOperationFailed),
        With<ToolOperationFinished>,
    >,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ToolOperationKind, ToolProvider};

    #[test]
    fn tools_page_ready_registers_state_subscriber() {
        let mut app = App::new();
        app.add_observer(page_ready);
        let webview = app
            .world_mut()
            .spawn(PageMetadata {
                url: "vmux://tools/npm".to_string(),
                ..Default::default()
            })
            .id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: PageReady {},
        });
        app.update();

        assert!(app.world().get::<ToolSubscriber>(webview).is_some());
    }

    #[test]
    fn tools_navigation_targets_the_emitting_page_stack() {
        let mut app = App::new();
        app.add_message::<PageOpenRequest>()
            .add_observer(navigate_request);
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ToolsNavigateRequest {
                url: "vmux://tools/lsp".to_string(),
            },
        });

        let messages = app.world().resource::<Messages<PageOpenRequest>>();
        let mut cursor = messages.get_cursor();
        let requests = cursor.read(messages).collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "vmux://tools/lsp");
        assert!(matches!(
            requests[0].target,
            PageOpenTarget::ContainingStack(target) if target == webview
        ));
    }

    #[test]
    fn tool_operation_state_tracks_pending_and_completion() {
        let operation = ToolOperationKey::new(
            ToolProvider::new("npm"),
            ToolOperationKind::Install,
            "typescript",
        );
        let mut subscriber = ToolSubscriber::pending(7, operation.clone());

        assert_eq!(
            subscriber.state.pending,
            vec![ToolOperationKey::new(
                ToolProvider::new("npm"),
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
}
