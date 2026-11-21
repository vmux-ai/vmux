use std::collections::BTreeMap;
use std::path::Path;
use std::result::Result;
use std::task::Waker;

use crate::catalog::{Catalog, Inventory, ProjectionSet};
use crate::inventory::{
    InventorySet, PendingToolOperation, Registry as ToolRegistry, Scan,
    Sequence as OperationRequestSequence,
};
use crate::state::{
    ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest, ToolInstallRequest,
    ToolLinkRequest, ToolOpenRequest, ToolOperationKey, ToolOperationKind, ToolOperationNotice,
    ToolUninstallRequest, ToolUnlinkRequest, ToolUpdateRequest, ToolsFilterRequest,
    ToolsNavigateRequest, ToolsRefreshRequest, ToolsSnapshot, ToolsUiState, ToolsView,
};
use bevy::prelude::*;
use bevy::prelude::{Commands, On, Query};
use bevy::tasks::IoTaskPool;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    AsyncTask, EntityTarget, OperationState, PageMetadata, PageOpenRequest, PageOpenTarget,
    TaskPlugin, TaskResult, TaskSet, TaskWaker, UiState, UiStatePlugin, UiStateWrite,
};

use crate::{
    ExternalToolOperation, ToolInventoryChanged, ToolOperationFailed, ToolOperationFinished,
    ToolOperationRequest, ToolOperationSucceeded, ToolOperator, ToolProviderId, ToolProviderTarget,
    ToolStore, ToolStoreTarget, ToolTaskSet,
};

pub(crate) struct ToolHostPlugin;

impl Plugin for ToolHostPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
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
        .add_systems(
            Update,
            (
                start_operation,
                (
                    start_external_operations::<ToolInstallRequest>,
                    start_external_operations::<ToolUpdateRequest>,
                    start_external_operations::<ToolUninstallRequest>,
                    start_external_operations::<ToolAdoptRequest>,
                    start_external_operations::<ToolImportRequest>,
                )
                    .in_set(ToolTaskSet::Start),
                finish_external_operations.in_set(ToolTaskSet::Apply),
                drain_succeeded_operations,
                drain_failed_operations,
                drain_canceled_operations,
                emit_tools_state.in_set(ProjectionSet),
            )
                .chain()
                .after(InventorySet),
        );
    }
}

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
struct ExternalWorker;

type ToolProviderOperationTask = AsyncTask<Result<String, String>>;

#[derive(Component)]
struct ToolOperationContext {
    operation_id: u64,
    target: Entity,
    operation: ToolOperationKey,
}

fn refresh_request(
    trigger: On<UiInput<ToolsRefreshRequest>>,
    registry: Query<Entity, With<ToolRegistry>>,
    subscribers: Query<(), With<ToolSubscriber>>,
    mut commands: Commands,
) {
    let Ok(registry) = registry.single() else {
        return;
    };
    if !subscribers.contains(trigger.event().webview) {
        commands
            .entity(trigger.event().webview)
            .insert(ToolSubscriber::default());
    }
    commands.trigger(crate::inventory::RefreshRequest {
        entity: registry,
        refresh_catalogs: trigger.event().payload.refresh,
    });
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

fn start_external_operations<R: Clone + Send + Sync + 'static>(
    operations: Query<
        (
            Entity,
            &ToolOperationRequest<R>,
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
    stores: Query<&ToolStore>,
    mut commands: Commands,
) where
    ToolOperationKey: From<R>,
{
    for (entity, request, target, adopt, import, state) in &operations {
        if !state
            .is_some_and(|state| matches!(state, OperationState::Pending | OperationState::Running))
        {
            continue;
        }
        let operation = ToolOperationKey::from(request.0.clone());
        let value = match operation.kind {
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
        let Some((_, operator)) = providers
            .iter()
            .find(|(provider, _)| provider.0 == operation.provider)
        else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed(format!(
                    "{} does not support this operation",
                    operation.provider.id()
                )),
            ));
            continue;
        };
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
                ExternalWorker,
                OperationState::Running,
            ));
    }
}

fn finish_external_operations(
    mut operations: Query<(Entity, &mut TaskResult<Result<String, String>>), With<ExternalWorker>>,
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
        (
            Entity,
            &ToolOperationContext,
            &ToolOperationSucceeded,
            Has<crate::package::Published>,
        ),
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
    for (entity, operation, completion, published) in &operations {
        if let Ok(mut subscriber) = subscribers.get_mut(operation.target) {
            subscriber.complete(
                operation.operation_id,
                operation.operation.clone(),
                true,
                completion.0.clone(),
            );
        }
        if !published {
            state.dirty = true;
            state.generation = state.generation.wrapping_add(1);
            changed.write(ToolInventoryChanged(operation.operation.provider.clone()));
        }
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
