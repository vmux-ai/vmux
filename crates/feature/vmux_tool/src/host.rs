use std::collections::BTreeMap;
use std::path::Path;
use std::result::Result;
use std::task::Waker;

use crate::catalog::{Catalog, Inventory, Received};
use crate::state::{
    ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest, ToolInstallRequest,
    ToolLinkRequest, ToolOpenRequest, ToolOperationKey, ToolOperationKind, ToolOperationNotice,
    ToolProvider, ToolProviderMetadata, ToolStatus, ToolUninstallRequest, ToolUnlinkRequest,
    ToolUpdateRequest, ToolsFilterRequest, ToolsNavigateRequest, ToolsRefreshRequest,
    ToolsSnapshot, ToolsUiState, ToolsView,
};
use bevy::prelude::*;
use bevy::prelude::{Commands, On, Query};
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    AsyncTask, EntityTarget, Operation, OperationRetained, OperationState, OperationTarget,
    PageMetadata, PageOpenRequest, PageOpenTarget, TaskPlugin, TaskResult, TaskSet, TaskWaker,
    UiState, UiStatePlugin, UiStateWrite,
};
use vmux_feature::manifest::{FeatureId, FeatureManifest};

use crate::storage::{ReadRequest, ReadResult, WriteRequest, WriteResult};
use crate::{
    ExternalToolOperation, ToolApplier, ToolInventoryChanged, ToolOperationFailed,
    ToolOperationFinished, ToolOperationRequest, ToolOperationSucceeded, ToolOperator,
    ToolProviderBinding, ToolProviderId, ToolProviderTarget, ToolScanner, ToolStore,
    ToolStoreTarget, ToolTaskSet, ToolsManifest,
};

pub(crate) struct ToolHostPlugin;

impl Plugin for ToolHostPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::catalog::CatalogPlugin,
            crate::McpConnectionPlugin,
            TaskPlugin::<Result<String, String>>::default(),
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
        .configure_sets(Update, ProjectionSet)
        .configure_sets(
            Update,
            TaskSet::of::<Result<String, String>>().in_set(ToolTaskSet::Collect),
        )
        .add_systems(Update, wake.in_set(ToolTaskSet::Wake))
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
        .add_observer(complete)
        .add_systems(Startup, spawn_registry)
        .add_systems(
            Update,
            (
                bind,
                ApplyDeferred,
                request_scan,
                load_scans,
                advance_scans,
                poll_scans,
                finish_scans,
                commit_scans,
                finish_commits,
                start_operation,
                start_external_operations.in_set(ToolTaskSet::Start),
                finish_external_operations.in_set(ToolTaskSet::Apply),
                drain_succeeded_operations,
                drain_failed_operations,
                drain_canceled_operations,
                emit_tools_state.in_set(ProjectionSet),
            )
                .chain(),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ProjectionSet;

fn wake(
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut tasks: Query<&mut TaskWaker>,
    stores: Query<&ToolStore>,
    mut notification: Local<Option<Waker>>,
) {
    let Some(proxy) = proxy else {
        return;
    };
    if notification.is_none() || proxy.is_changed() {
        *notification = Some(vmux_ecs::wake::Wake::waker(Some(&proxy)));
    }
    let notification = notification.as_ref().unwrap();
    for store in &stores {
        store.bind_waker(notification.clone());
    }
    for mut waker in &mut tasks {
        if !waker.is_bound() {
            waker.bind(notification.clone());
        }
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
    if !page.url.starts_with(crate::ToolPlugin::url()) {
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
    stores: Query<&ToolStore, With<ToolRegistry>>,
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
    mut pages: Query<&mut PageMetadata>,
) {
    let Some(route) = crate::route::ToolRoute::from_request(&trigger.event().payload) else {
        return;
    };
    let webview = trigger.event().webview;
    if let Ok(mut subscriber) = subscribers.get_mut(webview) {
        subscriber.navigate(&route.url());
    }
    if let Ok(mut page) = pages.get_mut(webview) {
        page.url = route.url();
    }
}

fn spawn_registry(registry: Query<Entity, With<ToolRegistry>>, mut commands: Commands) {
    if !registry.is_empty() {
        return;
    }
    commands.spawn((
        Name::new("Tool registry"),
        ToolRegistry::default(),
        Catalog::default(),
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
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            dirty: true,
            refresh_catalogs: false,
            generation: 1,
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

    fn cancel(&mut self, operation_id: u64) {
        if self.pending.remove(&operation_id).is_none() {
            return;
        }
        self.state.pending = self.pending.values().cloned().collect();
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
            items.sort_by(|left, right| {
                left.status
                    .display_order()
                    .cmp(&right.status.display_order())
                    .then_with(|| right.managed.cmp(&left.managed))
                    .then_with(|| {
                        left.name
                            .to_ascii_lowercase()
                            .cmp(&right.name.to_ascii_lowercase())
                    })
            });
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
struct Scan {
    generation: u64,
    refresh_catalogs: bool,
}

#[derive(Component)]
struct ScanManifest {
    original: Option<ToolsManifest>,
    desired: ToolsManifest,
}

#[derive(Component)]
struct ScanSnapshot(ToolsSnapshot);

impl ScanSnapshot {
    fn error(&mut self, error: &str) {
        if !self.0.error.is_empty() {
            self.0.error.push('\n');
        }
        self.0.error.push_str(error);
    }

    fn count(&mut self) {
        let mut installed = 0;
        let mut updates = 0;
        let mut conflicts = 0;
        for category in &self.0.categories {
            for item in &category.items {
                match item.status {
                    ToolStatus::Installed => installed += 1,
                    ToolStatus::Outdated => {
                        installed += 1;
                        updates += 1;
                    }
                    ToolStatus::Conflict => conflicts += 1,
                    _ => {}
                }
            }
        }
        self.0.installed = installed;
        self.0.updates = updates;
        self.0.conflicts = conflicts;
    }

    fn management(&mut self, committed: &ToolsManifest) {
        for category in &mut self.0.categories {
            for item in &mut category.items {
                item.managed = committed.contains(item.provider.id(), &item.id);
                if !item.managed
                    && matches!(item.status, ToolStatus::Installed | ToolStatus::Outdated)
                    && !item.operations.contains(&ToolOperationKind::Adopt)
                {
                    item.operations.insert(0, ToolOperationKind::Adopt);
                }
            }
        }
    }
}

#[derive(Component)]
struct ScanProvider {
    id: ToolProviderId,
}

#[derive(Component)]
struct Observed;

#[derive(Component)]
struct ScanTask(Task<ScanObservation>);

#[derive(Component)]
struct ScanObservation {
    manifest: ToolsManifest,
    result: Result<crate::state::ToolCategory, String>,
    errors: Vec<String>,
}

type ToolProviderOperationTask = AsyncTask<Result<String, String>>;

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
    mut registry: Query<(&mut ToolRegistry, &mut Catalog)>,
    subscribers: Query<(), With<ToolSubscriber>>,
    mut commands: Commands,
) {
    let Ok((mut state, mut catalog)) = registry.single_mut() else {
        return;
    };
    let request = &trigger.event().payload;
    if !subscribers.contains(trigger.event().webview) {
        commands
            .entity(trigger.event().webview)
            .insert(ToolSubscriber::default());
    }
    if request.refresh || !catalog.loaded {
        state.dirty = true;
        state.refresh_catalogs |= request.refresh;
        state.generation = state.generation.wrapping_add(1);
        if request.refresh && catalog.loaded {
            catalog.loaded = false;
            catalog.revision = catalog.revision.wrapping_add(1);
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
    pending: Query<
        (Entity, &ToolOperationContext),
        (With<PendingToolOperation>, Without<ToolOperationFinished>),
    >,
    active: Query<&OperationState, With<EntityTarget<ToolStore>>>,
    scans: Query<(), With<Scan>>,
    stores: Query<(Entity, &ToolStore), With<ToolRegistry>>,
    providers: Query<(Entity, &ToolProviderId)>,
    mut commands: Commands,
) {
    if active
        .iter()
        .any(|state| matches!(state, OperationState::Pending | OperationState::Running))
        || !scans.is_empty()
    {
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
    let Ok((store, storage)) = stores.single() else {
        return;
    };
    if storage.is_busy() {
        return;
    }
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
    mut registry: Query<(Entity, &mut ToolRegistry, &mut Catalog, &ToolStore)>,
    providers: Query<(Entity, &ToolProviderId, &ToolScanner)>,
    features: Query<&FeatureManifest>,
    scans: Query<(), With<Scan>>,
    tool_operations: Query<&OperationState, With<EntityTarget<ToolStore>>>,
    pending_tool_operations: Query<(), With<PendingToolOperation>>,
    mut commands: Commands,
) {
    let Ok((registry_entity, mut state, mut catalog, store)) = registry.single_mut() else {
        return;
    };
    if !state.dirty
        || store.is_busy()
        || !scans.is_empty()
        || tool_operations
            .iter()
            .any(|state| matches!(state, OperationState::Pending | OperationState::Running))
        || !pending_tool_operations.is_empty()
    {
        return;
    }
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
    if catalog.providers != metadata {
        catalog.providers = metadata.clone();
        catalog.revision = catalog.revision.wrapping_add(1);
    }
    let scan = commands
        .spawn((
            Name::new("Tool inventory scan"),
            Operation::bundle("tool.inventory").unwrap().running(),
            OperationTarget::new(registry_entity),
            OperationRetained,
            store.clone(),
            ReadRequest { retain_lease: true },
            Scan {
                generation: state.generation,
                refresh_catalogs: state.refresh_catalogs,
            },
            ScanSnapshot(ToolsSnapshot {
                loaded: true,
                root: store.root().to_string_lossy().into_owned(),
                providers: metadata,
                ..Default::default()
            }),
        ))
        .id();
    for (entity, id, _) in &providers {
        commands.spawn((
            Name::new(format!("Observe {}", id.0.id())),
            ChildOf(scan),
            ToolProviderTarget::new(entity),
            ScanProvider { id: id.clone() },
        ));
    }
    state.dirty = false;
    state.refresh_catalogs = false;
}

fn load_scans(
    mut scans: Query<
        (
            Entity,
            &ReadResult,
            &OperationState,
            Option<&OperationTarget>,
            &mut ScanSnapshot,
        ),
        With<Scan>,
    >,
    registry: Query<&ToolsManifest, With<ToolRegistry>>,
    mut commands: Commands,
) {
    for (entity, result, state, target, mut snapshot) in &mut scans {
        commands.entity(entity).remove::<ReadResult>();
        let Some(target) = target.filter(|target| registry.contains(target.entity())) else {
            commands.entity(entity).despawn();
            continue;
        };
        if *state != OperationState::Running {
            commands.entity(entity).despawn();
            continue;
        }
        let manifest = match &result.0 {
            Ok(manifest) => ScanManifest {
                original: Some(manifest.clone()),
                desired: manifest.clone(),
            },
            Err(error) => {
                snapshot.error(error);
                ScanManifest {
                    original: None,
                    desired: registry.get(target.entity()).unwrap().clone(),
                }
            }
        };
        commands.entity(entity).insert(manifest);
    }
}

fn advance_scans(
    scans: Query<(Entity, &Scan, &ScanManifest, &ToolStore, &OperationState)>,
    pending: Query<
        (
            Entity,
            &ChildOf,
            Option<&EntityTarget<ToolProviderId>>,
            &ScanProvider,
            Has<ScanTask>,
            Has<ScanObservation>,
        ),
        Without<Observed>,
    >,
    providers: Query<(&ToolProviderId, &ToolScanner)>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, scan, manifest, store, state) in &scans {
        let mut selected = None;
        let mut busy = false;
        for (child, parent, target, provider, running, observed) in &pending {
            if parent.parent() != entity {
                continue;
            }
            if running || observed {
                busy = true;
                break;
            }
            if *state != OperationState::Running {
                commands.entity(child).despawn();
                continue;
            }
            if selected.as_ref().is_none_or(
                |(_, _, current): &(
                    Entity,
                    Option<&EntityTarget<ToolProviderId>>,
                    &ScanProvider,
                )| { provider.id.0 < current.id.0 },
            ) {
                selected = Some((child, target, provider));
            }
        }
        if busy {
            continue;
        }
        let Some((child, target, provider)) = selected else {
            continue;
        };
        let current = target.and_then(|target| providers.get(target.entity()).ok());
        let Some((_, scanner)) = current.filter(|(id, _)| **id == provider.id) else {
            commands.entity(child).insert(ScanObservation {
                manifest: manifest.desired.clone(),
                result: Err("tool provider is unavailable".into()),
                errors: Vec::new(),
            });
            continue;
        };
        let store = store.clone();
        let mut manifest = manifest.desired.clone();
        let scanner = *scanner;
        let provider = provider.id.clone();
        let refresh = scan.refresh_catalogs;
        let wake = wake.as_deref().map(|wake| wake.0.clone());
        commands
            .entity(child)
            .insert(ScanTask(IoTaskPool::get().spawn(async move {
                let (result, errors) =
                    match scanner.scan(&provider.0, &store, &mut manifest, refresh) {
                        Ok(snapshot) => (Ok(snapshot.category), snapshot.errors),
                        Err(error) => (Err(error), Vec::new()),
                    };
                if let Some(wake) = wake {
                    wake.wake();
                }
                ScanObservation {
                    manifest,
                    result,
                    errors,
                }
            })));
    }
}

fn poll_scans(mut tasks: Query<(Entity, &mut ScanTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<ScanTask>().insert(result);
    }
}

fn finish_scans(
    observations: Query<(
        Entity,
        &ChildOf,
        Option<&EntityTarget<ToolProviderId>>,
        &ScanProvider,
        &ScanObservation,
    )>,
    providers: Query<&ToolProviderId>,
    mut scans: Query<(&OperationState, &mut ScanManifest, &mut ScanSnapshot)>,
    mut commands: Commands,
) {
    for (entity, parent, target, provider, observation) in &observations {
        let Ok((state, mut manifest, mut snapshot)) = scans.get_mut(parent.parent()) else {
            commands.entity(entity).despawn();
            continue;
        };
        if *state != OperationState::Running {
            commands.entity(entity).despawn();
            continue;
        }
        if target.and_then(|target| providers.get(target.entity()).ok()) != Some(&provider.id) {
            commands.entity(entity).despawn();
            snapshot.error(&format!(
                "{}: tool provider is unavailable",
                provider.id.0.id()
            ));
            continue;
        }
        match &observation.result {
            Ok(result) => {
                commands
                    .entity(entity)
                    .remove::<ScanObservation>()
                    .insert(Observed);
                manifest.desired.clone_from(&observation.manifest);
                snapshot.0.categories.push(result.clone());
                for error in &observation.errors {
                    snapshot.error(error);
                }
            }
            Err(error) => {
                commands.entity(entity).despawn();
                snapshot.error(&format!("{}: {error}", provider.id.0.id()));
            }
        }
    }
}

fn commit_scans(
    mut scans: Query<
        (
            Entity,
            &Scan,
            Option<&OperationTarget>,
            &OperationState,
            &ToolStore,
            &ScanManifest,
            &mut ScanSnapshot,
        ),
        (Without<WriteRequest>, Without<WriteResult>),
    >,
    pending: Query<(
        &ChildOf,
        &ScanProvider,
        Option<&EntityTarget<ToolProviderId>>,
        Has<Observed>,
    )>,
    providers: Query<&ToolProviderId>,
    mut registry: Query<(&mut ToolRegistry, &ToolStore, &mut ToolsManifest)>,
    mut commands: Commands,
) {
    for (entity, scan, target, state, store, manifest, mut snapshot) in &mut scans {
        if pending
            .iter()
            .any(|(parent, _, _, observed)| parent.parent() == entity && !observed)
        {
            continue;
        }
        let Some(target) = target else {
            commands.entity(entity).despawn();
            continue;
        };
        let Ok((mut registry, current_store, mut committed)) = registry.get_mut(target.entity())
        else {
            commands.entity(entity).despawn();
            continue;
        };
        if *state != OperationState::Running
            || scan.generation != registry.generation
            || store != current_store
            || pending.iter().any(|(parent, provider, target, _)| {
                parent.parent() == entity
                    && target.and_then(|target| providers.get(target.entity()).ok())
                        != Some(&provider.id)
            })
        {
            registry.dirty = true;
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(original) = &manifest.original {
            if original != &manifest.desired {
                commands.entity(entity).insert(WriteRequest {
                    manifest: manifest.desired.clone(),
                    retain_lease: true,
                });
                continue;
            }
            committed.clone_from(&manifest.desired);
        }
        snapshot.management(&committed);
        snapshot.count();
        commands.trigger(Received {
            entity: target.entity(),
            snapshot: snapshot.0.clone(),
        });
        commands.entity(entity).despawn();
    }
}

fn finish_commits(
    mut scans: Query<(
        Entity,
        &Scan,
        Option<&OperationTarget>,
        &OperationState,
        &ToolStore,
        &ScanManifest,
        &mut ScanSnapshot,
        &WriteResult,
    )>,
    observed: Query<(
        &ChildOf,
        &ScanProvider,
        Option<&EntityTarget<ToolProviderId>>,
    )>,
    providers: Query<&ToolProviderId>,
    mut registry: Query<(&mut ToolRegistry, &ToolStore, &mut ToolsManifest)>,
    mut commands: Commands,
) {
    for (entity, scan, target, state, store, manifest, mut snapshot, result) in &mut scans {
        commands.entity(entity).despawn();
        let Some(target) = target else {
            continue;
        };
        let Ok((mut registry, current_store, mut committed)) = registry.get_mut(target.entity())
        else {
            continue;
        };
        if *state != OperationState::Running
            || scan.generation != registry.generation
            || store != current_store
            || observed.iter().any(|(parent, provider, target)| {
                parent.parent() == entity
                    && target.and_then(|target| providers.get(target.entity()).ok())
                        != Some(&provider.id)
            })
        {
            registry.dirty = true;
            continue;
        }
        match &result.0 {
            Ok(()) => committed.clone_from(&manifest.desired),
            Err(error) => {
                snapshot.error(&format!("Tools: {error}"));
                if let Some(original) = &manifest.original {
                    committed.clone_from(original);
                }
            }
        }
        snapshot.management(&committed);
        snapshot.count();
        commands.trigger(Received {
            entity: target.entity(),
            snapshot: snapshot.0.clone(),
        });
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
            Option<&OperationState>,
        ),
        (
            With<ExternalToolOperation>,
            Without<ToolProviderOperationTask>,
            Without<ToolOperationFinished>,
        ),
    >,
    providers: Query<(&ToolProviderId, &ToolOperator)>,
    appliers: Query<&ToolApplier>,
    registry: Query<Entity, With<ToolRegistry>>,
    inventory: Inventory,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, context, target, adopt, import, state) in &operations {
        if !state
            .is_some_and(|state| matches!(state, OperationState::Pending | OperationState::Running))
        {
            continue;
        }
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
        let Some(lease) = store.lease() else {
            continue;
        };
        if context.operation.kind == ToolOperationKind::Apply {
            let Ok(registry) = registry.single() else {
                continue;
            };
            let Some(snapshot) = inventory.snapshot(registry) else {
                continue;
            };
            let operators = providers
                .iter()
                .map(|(provider, operator)| (provider.clone(), *operator))
                .collect::<Vec<_>>();
            let installs = snapshot
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
            let task = IoTaskPool::get().spawn(async move {
                let _lease = lease;
                apply_tools(&store, &installs, &operators, &appliers)
            });
            commands
                .entity(entity)
                .remove::<ExternalToolOperation>()
                .insert((
                    ToolProviderOperationTask::new(task),
                    OperationState::Running,
                ));
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
        let task = IoTaskPool::get().spawn(async move {
            let _lease = lease;
            operator.run(&store, &operation, value.as_str())
        });
        commands
            .entity(entity)
            .remove::<ExternalToolOperation>()
            .insert((
                ToolProviderOperationTask::new(task),
                OperationState::Running,
            ));
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
    mut operations: Query<
        (Entity, &mut TaskResult<Result<String, String>>),
        With<ToolOperationContext>,
    >,
    mut commands: Commands,
) {
    for (entity, mut completion) in &mut operations {
        let Some(result) = completion.take() else {
            continue;
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => Err(error.to_string()),
        };
        commands.queue(completion.guard(
            entity,
            ProviderCompletion {
                entity,
                result: Some(result),
            },
        ));
    }
}

#[derive(Event)]
struct ProviderCompletion {
    entity: Entity,
    result: Option<Result<String, String>>,
}

impl Command for ProviderCompletion {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.trigger(self);
    }
}

fn complete(
    mut receipt: On<ProviderCompletion>,
    operations: Query<(&OperationState, Option<&EntityTarget<ToolStore>>)>,
    stores: Query<(), With<ToolStore>>,
    mut commands: Commands,
) {
    let receipt = receipt.event_mut();
    let Ok((state, target)) = operations.get(receipt.entity) else {
        return;
    };
    if !matches!(state, OperationState::Pending | OperationState::Running) {
        return;
    }
    if target.is_some_and(|target| !stores.contains(target.entity())) {
        commands
            .entity(receipt.entity)
            .insert(OperationState::Canceled);
        return;
    }
    let Some(result) = receipt.result.take() else {
        return;
    };
    match result {
        Ok(message) => {
            commands
                .entity(receipt.entity)
                .insert((ToolOperationFinished, ToolOperationSucceeded(message)));
        }
        Err(error) => {
            commands
                .entity(receipt.entity)
                .insert((ToolOperationFinished, ToolOperationFailed(error)));
        }
    }
}

fn drain_succeeded_operations(
    operations: Query<
        (Entity, &ToolOperationContext, &ToolOperationSucceeded),
        With<ToolOperationFinished>,
    >,
    mut registry: Query<&mut ToolRegistry>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut changed: MessageWriter<ToolInventoryChanged>,
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
        changed.write(ToolInventoryChanged(operation.operation.provider.clone()));
        commands.entity(entity).remove::<ToolOperationContext>();
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
        commands.entity(entity).remove::<ToolOperationContext>();
    }
}

fn drain_canceled_operations(
    operations: Query<(Entity, &ToolOperationContext, &OperationState)>,
    mut subscribers: Query<&mut ToolSubscriber>,
    mut commands: Commands,
) {
    for (entity, operation, state) in &operations {
        if *state != OperationState::Canceled {
            continue;
        }
        if let Ok(mut subscriber) = subscribers.get_mut(operation.target) {
            subscriber.cancel(operation.operation_id);
        }
        commands
            .entity(entity)
            .remove::<(ToolOperationContext, PendingToolOperation)>();
    }
}

fn emit_tools_state(
    registry: Query<(Entity, &Catalog), With<ToolRegistry>>,
    inventory: Inventory,
    mut subscribers: Query<(Entity, &mut ToolSubscriber)>,
    mut commands: Commands,
) {
    let Ok((registry, catalog)) = registry.single() else {
        return;
    };
    let snapshot = if subscribers
        .iter()
        .any(|(_, subscriber)| subscriber.snapshot_revision != catalog.revision)
    {
        inventory.snapshot(registry)
    } else {
        None
    };
    for (entity, mut subscriber) in &mut subscribers {
        if let Some(snapshot) = &snapshot {
            subscriber.synchronize(catalog.revision, snapshot);
        }
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
    fn queued_provider_completion_refuses_cancellation_and_retired_stores() {
        for interruption in ["none", "cancel", "store"] {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins((crate::ToolRuntimePlugin, ToolHostPlugin));
            let store = app
                .world_mut()
                .spawn(ToolStore::new(directory.path(), directory.path()))
                .id();
            let operation = app
                .world_mut()
                .spawn((
                    vmux_ecs::Operation::bundle("tools.test.provider")
                        .unwrap()
                        .running(),
                    ToolStoreTarget::new(store),
                ))
                .id();
            app.world_mut().commands().queue(ProviderCompletion {
                entity: operation,
                result: Some(Ok("completed".into())),
            });
            match interruption {
                "none" => {}
                "cancel" => {
                    app.world_mut()
                        .entity_mut(operation)
                        .insert(OperationState::Canceled);
                }
                "store" => {
                    app.world_mut().despawn(store);
                }
                _ => unreachable!(),
            }
            app.world_mut().flush();
            if interruption == "none" {
                assert_eq!(
                    app.world()
                        .get::<ToolOperationSucceeded>(operation)
                        .map(|result| result.0.as_str()),
                    Some("completed")
                );
                assert!(
                    app.world()
                        .get::<ToolOperationFinished>(operation)
                        .is_some()
                );
            } else {
                assert_eq!(
                    app.world().get::<OperationState>(operation),
                    Some(&OperationState::Canceled)
                );
                assert!(
                    app.world()
                        .get::<ToolOperationSucceeded>(operation)
                        .is_none()
                );
                assert!(
                    app.world()
                        .get::<ToolOperationFinished>(operation)
                        .is_none()
                );
            }
        }
    }

    struct InventoryFixture {
        app: App,
        directory: tempfile::TempDir,
        registry: Entity,
        provider: Entity,
    }

    impl InventoryFixture {
        fn new(source: &str, hold: bool) -> Self {
            IoTaskPool::get_or_init(|| bevy::tasks::TaskPoolBuilder::new().num_threads(2).build());
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(directory.path(), directory.path());
            std::fs::create_dir_all(store.root()).unwrap();
            std::fs::write(store.manifest_path(), source).unwrap();
            if hold {
                std::fs::write(store.root().join("hold"), "").unwrap();
            }
            let mut app = App::new();
            app.add_plugins((crate::ToolRuntimePlugin, ToolHostPlugin));
            let registry = app
                .world_mut()
                .spawn((
                    Name::new("Fixture tool registry"),
                    ToolRegistry::default(),
                    Catalog::default(),
                    store,
                    ToolsManifest::default(),
                    OperationRequestSequence::default(),
                ))
                .id();
            let provider = app
                .world_mut()
                .spawn((
                    ToolProviderId(ToolProvider::new("fixture")),
                    ToolScanner::new(Self::observe),
                ))
                .id();
            app.update();
            Self {
                app,
                directory,
                registry,
                provider,
            }
        }

        fn observe(
            provider: &ToolProvider,
            store: &ToolStore,
            manifest: &mut ToolsManifest,
            _: bool,
        ) -> Result<crate::ToolProviderSnapshot, String> {
            std::fs::write(store.root().join("started"), "").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while store.root().join("hold").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture observation timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            manifest.set_package(provider.id(), "observed", true);
            if store.root().join("fail").exists() {
                return Err("fixture provider failed".into());
            }
            Ok(crate::state::ToolCategory {
                provider: provider.clone(),
                items: vec![crate::state::ToolItem {
                    provider: provider.clone(),
                    id: "observed".into(),
                    name: "Observed".into(),
                    icon: None,
                    version: Some("1".into()),
                    detail: String::new(),
                    status: ToolStatus::Installed,
                    managed: true,
                    operations: vec![ToolOperationKind::Uninstall],
                }],
            }
            .into())
        }

        fn scan(&mut self) -> Entity {
            self.app
                .world_mut()
                .query_filtered::<Entity, With<Scan>>()
                .single(self.app.world())
                .unwrap()
        }

        fn wait_started(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !self.directory.path().join("tools/started").exists() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture scan did not start"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        fn drain(&mut self, entity: Entity) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.app.world().get_entity(entity).is_ok() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture scan did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        fn snapshot(&mut self) -> ToolsSnapshot {
            let mut state = bevy::ecs::system::SystemState::<Inventory>::new(self.app.world_mut());
            state
                .get(self.app.world())
                .unwrap()
                .snapshot(self.registry)
                .unwrap()
        }
    }

    #[test]
    fn inventory_commits_observed_packages_after_read_and_preserves_existing_declarations() {
        let mut fixture =
            InventoryFixture::new("version = 1\n[packages]\nfixture = ['existing']\n", false);
        let scan = fixture.scan();
        fixture.drain(scan);
        let store = fixture
            .app
            .world()
            .get::<ToolStore>(fixture.registry)
            .unwrap();
        let saved: ToolsManifest =
            toml::from_str(&std::fs::read_to_string(store.manifest_path()).unwrap()).unwrap();
        assert!(saved.contains("fixture", "existing"));
        assert!(saved.contains("fixture", "observed"));
        assert!(store.lease().is_some());
        assert!(
            fixture
                .app
                .world()
                .get::<ToolsManifest>(fixture.registry)
                .unwrap()
                .contains("fixture", "observed")
        );
        let snapshot = fixture.snapshot();
        assert_eq!(snapshot.installed, 1);
        assert!(snapshot.categories[0].items[0].managed);
        assert!(snapshot.error.is_empty());
    }

    #[test]
    fn obsolete_inventory_results_cannot_save_or_transfer_to_replacement_owners() {
        for interruption in ["generation", "store", "registry", "provider", "cancel"] {
            let source = "version = 1\n[packages]\nfixture = ['existing']\n";
            let mut fixture = InventoryFixture::new(source, true);
            let scan = fixture.scan();
            fixture.wait_started();
            let store = fixture
                .app
                .world()
                .get::<ToolStore>(fixture.registry)
                .unwrap()
                .clone();
            assert!(store.lease().is_none());
            match interruption {
                "generation" => {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<ToolRegistry>(fixture.registry)
                        .unwrap()
                        .generation += 1;
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.registry)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.directory.path(),
                        ));
                }
                "registry" => {
                    fixture.app.world_mut().despawn(fixture.registry);
                }
                "provider" => {
                    fixture.app.world_mut().despawn(fixture.provider);
                }
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(scan)
                        .insert(OperationState::Canceled);
                }
                _ => unreachable!(),
            }
            fixture.app.update();
            assert!(store.lease().is_none(), "{interruption}");
            std::fs::remove_file(store.root().join("hold")).unwrap();
            fixture.drain(scan);
            assert_eq!(
                std::fs::read_to_string(store.manifest_path()).unwrap(),
                source,
                "{interruption}"
            );
            assert!(store.lease().is_some(), "{interruption}");
            if let Some(manifest) = fixture.app.world().get::<ToolsManifest>(fixture.registry) {
                assert!(!manifest.contains("fixture", "observed"), "{interruption}");
            }
        }
    }

    #[test]
    fn unreadable_inventory_never_overwrites_configuration_or_claims_managed_packages() {
        let source = "invalid = [";
        let mut fixture = InventoryFixture::new(source, false);
        let scan = fixture.scan();
        fixture.drain(scan);
        let store = fixture
            .app
            .world()
            .get::<ToolStore>(fixture.registry)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(store.manifest_path()).unwrap(),
            source
        );
        let snapshot = fixture.snapshot();
        assert!(!snapshot.error.is_empty());
        let item = &snapshot.categories[0].items[0];
        assert!(!item.managed);
        assert!(item.operations.contains(&ToolOperationKind::Adopt));
    }

    #[test]
    fn failed_inventory_providers_cannot_commit_partial_manifest_changes() {
        let source = "version = 1\n";
        let mut fixture = InventoryFixture::new(source, true);
        let scan = fixture.scan();
        fixture.wait_started();
        let store = fixture
            .app
            .world()
            .get::<ToolStore>(fixture.registry)
            .unwrap()
            .clone();
        std::fs::write(store.root().join("fail"), "").unwrap();
        std::fs::remove_file(store.root().join("hold")).unwrap();
        fixture.drain(scan);
        assert_eq!(
            std::fs::read_to_string(store.manifest_path()).unwrap(),
            source
        );
        assert!(
            !fixture
                .app
                .world()
                .get::<ToolsManifest>(fixture.registry)
                .unwrap()
                .contains("fixture", "observed")
        );
        let snapshot = fixture.snapshot();
        assert!(snapshot.error.contains("fixture provider failed"));
        assert!(snapshot.categories.is_empty());
    }

    #[test]
    fn accepted_inventory_writes_drain_before_releasing_the_store_lease() {
        for interruption in ["generation", "registry", "provider", "store", "cancel"] {
            let mut fixture = InventoryFixture::new("version = 1\n", false);
            let scan = fixture.scan();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while fixture.app.world().get::<WriteRequest>(scan).is_none() {
                fixture.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "scan did not request a write"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let store = fixture
                .app
                .world()
                .get::<ToolStore>(fixture.registry)
                .unwrap()
                .clone();
            assert!(store.lease().is_none(), "{interruption}");
            match interruption {
                "generation" => {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<ToolRegistry>(fixture.registry)
                        .unwrap()
                        .generation += 1;
                }
                "registry" => {
                    fixture.app.world_mut().despawn(fixture.registry);
                }
                "provider" => {
                    fixture.app.world_mut().despawn(fixture.provider);
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.registry)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.directory.path(),
                        ));
                }
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(scan)
                        .insert(OperationState::Canceled);
                }
                _ => unreachable!(),
            }
            fixture.drain(scan);
            assert!(store.lease().is_some(), "{interruption}");
            if let Some(manifest) = fixture.app.world().get::<ToolsManifest>(fixture.registry) {
                assert!(!manifest.contains("fixture", "observed"), "{interruption}");
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<ToolRegistry>(fixture.registry)
                        .unwrap()
                        .dirty
                );
            }
            let saved: ToolsManifest =
                toml::from_str(&std::fs::read_to_string(store.manifest_path()).unwrap()).unwrap();
            assert!(saved.contains("fixture", "observed"), "{interruption}");
        }
    }

    #[test]
    fn native_tool_task_stages_form_a_valid_production_schedule() {
        let mut app = App::new();
        app.add_plugins((crate::ToolRuntimePlugin, ToolHostPlugin));
        app.world_mut().schedule_scope(Update, |world, schedule| {
            schedule.initialize(world).unwrap();
        });
    }

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
    fn tools_navigation_keeps_the_emitting_page() {
        let mut app = App::new();
        app.add_observer(navigate_request);
        let mut subscriber = ToolSubscriber::default();
        subscriber.state.view.route = "acp".to_string();
        let webview = app
            .world_mut()
            .spawn((
                PageMetadata {
                    url: "vmux://tools/acp".to_string(),
                    ..Default::default()
                },
                subscriber,
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ToolsNavigateRequest {
                url: "vmux://tools/lsp".to_string(),
            },
        });

        let page = app.world().get::<PageMetadata>(webview).unwrap();
        let subscriber = app.world().get::<ToolSubscriber>(webview).unwrap();
        assert_eq!(page.url, "vmux://tools/lsp");
        assert_eq!(subscriber.state.view.route, "lsp");
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
