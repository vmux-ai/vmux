use std::result::Result;

use crate::catalog::ProjectionSet;
use crate::inventory::{
    InventorySet, PendingToolOperation, Registry as ToolRegistry, Scan,
    Sequence as OperationRequestSequence,
};
use crate::presentation::{Context as ToolOperationContext, State as ToolPageState};
use crate::state::{
    McpRegisterRequest, ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest,
    ToolInstallRequest, ToolLinkRequest, ToolOpenRequest, ToolOperationKey, ToolOperationKind,
    ToolOperationNotice, ToolProvider, ToolUninstallRequest, ToolUnlinkRequest, ToolUpdateRequest,
    ToolsFilterRequest, ToolsNavigateRequest, ToolsRefreshRequest, ToolsUiState,
};
use bevy::prelude::*;
use bevy::prelude::{Commands, On, Query};
use bevy::tasks::IoTaskPool;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::{
    AsyncTask, EntityTarget, OperationState, PageMetadata, TaskPlugin, TaskResult, TaskSet,
    TaskWaker, UiStatePlugin,
};

use crate::{
    ExternalToolOperation, ToolInventoryChanged, ToolOperationFailed, ToolOperationFinished,
    ToolOperationRequest, ToolOperationSucceeded, ToolOperator, ToolProviderId, ToolProviderTarget,
    ToolStore, ToolStoreTarget, ToolTaskSet,
};

pub(crate) struct ToolHostPlugin;

impl Plugin for ToolHostPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::wake::WakePlugin>() {
            app.add_plugins(vmux_ecs::wake::WakePlugin);
        }
        app.add_plugins((
            crate::McpConnectionPlugin,
            crate::presentation::ToolPagePlugin,
            crate::open::ToolOpenPlugin,
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
                McpRegisterRequest,
            )>::default(),
        ))
        .configure_sets(Update, ProjectionSet)
        .configure_sets(
            Update,
            TaskSet::of::<Result<String, String>>().in_set(ToolTaskSet::Collect),
        )
        .add_systems(Update, wake.in_set(ToolTaskSet::Wake))
        .add_observer(operation_request::<ToolInstallRequest>)
        .add_observer(operation_request::<ToolUpdateRequest>)
        .add_observer(operation_request::<ToolUninstallRequest>)
        .add_observer(operation_request::<ToolForgetRequest>)
        .add_observer(operation_request::<ToolAdoptRequest>)
        .add_observer(operation_request::<ToolLinkRequest>)
        .add_observer(operation_request::<ToolUnlinkRequest>)
        .add_observer(operation_request::<ToolApplyRequest>)
        .add_observer(operation_request::<ToolImportRequest>)
        .add_observer(register_mcp)
        .add_observer(forget_mcp_config)
        .add_systems(Update, cancel.in_set(crate::ToolOperationCancelSet))
        .add_observer(complete)
        .add_systems(
            Update,
            (
                (
                    start_external_operations::<ToolInstallRequest>,
                    start_external_operations::<ToolUpdateRequest>,
                    start_external_operations::<ToolUninstallRequest>,
                    start_external_operations::<ToolAdoptRequest>,
                    start_external_operations::<ToolImportRequest>,
                )
                    .in_set(ToolTaskSet::Start),
                finish_external_operations.in_set(ToolTaskSet::Apply),
                drain_mcp_mutations,
                drain_succeeded_operations,
                drain_failed_operations,
                drain_canceled_operations,
            )
                .chain()
                .after(InventorySet)
                .before(ProjectionSet),
        )
        .add_systems(PostUpdate, start_operation.after(vmux_ecs::JobSet::Cleanup));
    }
}

fn wake(
    notification: Option<Res<vmux_ecs::JobWake>>,
    mut tasks: Query<&mut TaskWaker>,
    stores: Query<&ToolStore>,
) {
    let Some(notification) = notification else {
        return;
    };
    for store in &stores {
        store.bind_waker(notification.0.clone());
    }
    for mut waker in &mut tasks {
        if !waker.is_bound() {
            waker.bind(notification.0.clone());
        }
    }
}

#[derive(Component)]
struct ExternalWorker;

#[derive(Component)]
struct McpPageMutation {
    target: Entity,
    generation: u64,
    registry: Entity,
    store: ToolStore,
    id: String,
    kind: ToolOperationKind,
}

fn register_mcp(
    trigger: On<UiInput<McpRegisterRequest>>,
    mut registries: Query<(Entity, &ToolStore, &mut OperationRequestSequence), With<ToolRegistry>>,
    mut pages: Query<&mut ToolPageState>,
    metadata: Query<&PageMetadata>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok(metadata) = metadata.get(target) else {
        return;
    };
    let Some(route) = crate::route::ToolRoute::parse(&metadata.url) else {
        return;
    };
    let Ok((registry, store, mut sequence)) = registries.single_mut() else {
        return;
    };
    let payload = &trigger.event().payload;
    let operation_id = sequence.next();
    let generation = match pages.get_mut(target) {
        Ok(mut page) => {
            page.bind(registry, store, operation_id);
            page.navigate(&route);
            page.begin();
            page.generation
        }
        Err(_) => {
            let mut page = ToolPageState::new(operation_id);
            page.bind(registry, store, operation_id);
            page.navigate(&route);
            commands.entity(target).insert(page);
            operation_id
        }
    };
    let id = payload.id.trim().to_string();
    let target_value = payload.target.trim().to_string();
    let args = payload
        .args
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let bearer = (!payload.bearer_token_env_var.trim().is_empty())
        .then(|| payload.bearer_token_env_var.trim().to_string());
    let spec = match payload.transport.trim().to_ascii_lowercase().as_str() {
        "http" => crate::McpServerSpec::Http {
            url: target_value,
            headers: Default::default(),
            header_env: Default::default(),
            bearer_token_env_var: bearer,
            auth: crate::McpAuthSpec::None,
            enabled: true,
            agents: Vec::new(),
        },
        "sse" => crate::McpServerSpec::Sse {
            url: target_value,
            headers: Default::default(),
            header_env: Default::default(),
            bearer_token_env_var: bearer,
            auth: crate::McpAuthSpec::None,
            enabled: true,
            agents: Vec::new(),
        },
        _ => crate::McpServerSpec::Stdio {
            command: target_value,
            args,
            env: Default::default(),
            cwd: None,
            enabled: true,
            agents: Vec::new(),
        },
    };
    commands.spawn((
        McpPageMutation {
            target,
            generation,
            registry,
            store: store.clone(),
            id: id.clone(),
            kind: ToolOperationKind::Adopt,
        },
        crate::McpSaveEntry {
            path: store.mcp_config_path(),
            id: id.clone(),
            entry: crate::McpServerOverride::Define(spec),
            summary: format!("{id} registered in mcp.ron"),
        },
    ));
}

fn forget_mcp_config(
    trigger: On<UiInput<ToolForgetRequest>>,
    registries: Query<&ToolStore, With<ToolRegistry>>,
    metadata: Query<&PageMetadata>,
    mut commands: Commands,
) {
    let payload = &trigger.event().payload;
    if !payload.provider.is("mcp") {
        return;
    }
    let Ok(metadata) = metadata.get(trigger.event().webview) else {
        return;
    };
    if crate::route::ToolRoute::parse(&metadata.url).is_none() {
        return;
    }
    let Ok(store) = registries.single() else {
        return;
    };
    commands.spawn(crate::McpDeleteEntry {
        path: store.mcp_config_path(),
        id: payload.id.clone(),
        summary: format!("{} removed from mcp.ron", payload.id),
        ignore_missing: true,
    });
}

fn drain_mcp_mutations(
    mutations: Query<(Entity, Option<&McpPageMutation>, &crate::McpMutationResult)>,
    mut registry: Query<(Entity, &ToolStore, &mut ToolRegistry)>,
    mut pages: Query<&mut ToolPageState>,
    mut commands: Commands,
) {
    let Ok((registry_entity, store, mut state)) = registry.single_mut() else {
        for (entity, _, _) in &mutations {
            commands.entity(entity).despawn();
        }
        return;
    };
    for (entity, page_mutation, result) in &mutations {
        let mut refresh = false;
        if let Some(mutation) = page_mutation {
            let (success, message) = match &result.0 {
                Ok(message) => {
                    refresh = true;
                    (true, message.clone())
                }
                Err(error) => (false, error.clone()),
            };
            if let Ok(mut page) = pages.get_mut(mutation.target) {
                let context = ToolOperationContext {
                    operation_id: 0,
                    target: mutation.target,
                    generation: mutation.generation,
                    operation: ToolOperationKey::new(
                        ToolProvider::new("mcp"),
                        mutation.kind,
                        mutation.id.clone(),
                    ),
                    registry: mutation.registry,
                    store: mutation.store.clone(),
                };
                page.complete(
                    &context,
                    ToolOperationNotice {
                        operation: context.operation.clone(),
                        success,
                        message,
                    },
                );
            }
            if refresh && mutation.registry == registry_entity && &mutation.store == store {
                state.dirty = true;
                state.generation = state.generation.wrapping_add(1);
            }
        } else if result.0.is_ok() {
            state.dirty = true;
            state.generation = state.generation.wrapping_add(1);
        }
        commands.entity(entity).despawn();
    }
}

type ToolProviderOperationTask = AsyncTask<Result<String, String>>;

fn operation_request<R>(
    trigger: On<UiInput<R>>,
    mut registries: Query<(Entity, &ToolStore, &mut OperationRequestSequence), With<ToolRegistry>>,
    mut pages: Query<&mut ToolPageState>,
    metadata: Query<&PageMetadata>,
    mut commands: Commands,
) where
    R: Clone + Send + Sync + 'static,
    ToolOperationKey: From<R>,
{
    let target = trigger.event().webview;
    let Ok(metadata) = metadata.get(target) else {
        return;
    };
    let Some(route) = crate::route::ToolRoute::parse(&metadata.url) else {
        return;
    };
    let Ok((registry, store, mut sequence)) = registries.single_mut() else {
        return;
    };
    let request = trigger.event().payload.clone();
    let operation = ToolOperationKey::from(request.clone());
    let operation_id = sequence.next();
    let generation = match pages.get_mut(target) {
        Ok(mut page) => {
            page.bind(registry, store, operation_id);
            page.navigate(&route);
            page.begin();
            page.generation
        }
        Err(_) => {
            let mut page = ToolPageState::new(operation_id);
            page.bind(registry, store, operation_id);
            page.navigate(&route);
            commands.entity(target).insert(page);
            operation_id
        }
    };
    commands.spawn((
        PendingToolOperation,
        ToolOperationContext {
            operation_id,
            target,
            generation,
            operation,
            registry,
            store: store.clone(),
        },
        ToolOperationRequest(request),
    ));
}

fn cancel(
    operations: Query<(Entity, &ToolOperationContext, &OperationState)>,
    stores: Query<&ToolStore, With<ToolRegistry>>,
    mut commands: Commands,
) {
    for (entity, context, state) in &operations {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if stores.get(context.registry).ok() != Some(&context.store) {
            commands.entity(entity).insert(OperationState::Canceled);
        }
    }
}

fn start_operation(
    pending: Query<
        (Entity, &ToolOperationContext, &OperationState),
        (With<PendingToolOperation>, Without<ToolOperationFinished>),
    >,
    active: Query<&OperationState, With<EntityTarget<ToolStore>>>,
    scans: Query<(), With<Scan>>,
    stores: Query<&ToolStore, With<ToolRegistry>>,
    providers: Query<(Entity, &ToolProviderId), With<crate::ToolProviderBinding>>,
    wake: Option<Res<vmux_ecs::JobWake>>,
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
    for (entity, operation, state) in &pending {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        match next {
            Some((_, operation_id, _)) if operation_id <= operation.operation_id => {}
            _ => next = Some((entity, operation.operation_id, operation)),
        }
    }
    let Some((entity, _, context)) = next else {
        return;
    };
    let Ok(storage) = stores.get(context.registry) else {
        return;
    };
    if storage != &context.store || storage.is_busy() {
        return;
    }
    let mut operation_entity = commands.entity(entity);
    operation_entity
        .remove::<PendingToolOperation>()
        .insert(ToolStoreTarget::new(context.registry));
    if let Some((provider, _)) = providers
        .iter()
        .find(|(_, provider)| provider.0 == context.operation.provider)
    {
        operation_entity.insert(ToolProviderTarget::new(provider));
    }
    if let Some(wake) = &wake {
        wake.0.wake_by_ref();
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
    mut registry: Query<(Entity, &ToolStore, &mut ToolRegistry)>,
    mut pages: Query<&mut ToolPageState>,
    mut changed: MessageWriter<ToolInventoryChanged>,
    mut commands: Commands,
) {
    let Ok((registry, store, mut state)) = registry.single_mut() else {
        return;
    };
    for (entity, operation, completion, published) in &operations {
        if let Ok(mut page) = pages.get_mut(operation.target) {
            page.complete(
                operation,
                ToolOperationNotice {
                    operation: operation.operation.clone(),
                    success: true,
                    message: completion.0.clone(),
                },
            );
        }
        if !published && operation.registry == registry && &operation.store == store {
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
    mut pages: Query<&mut ToolPageState>,
    mut commands: Commands,
) {
    for (entity, operation, failure) in &operations {
        if let Ok(mut page) = pages.get_mut(operation.target) {
            page.complete(
                operation,
                ToolOperationNotice {
                    operation: operation.operation.clone(),
                    success: false,
                    message: failure.0.clone(),
                },
            );
        }
        commands.entity(entity).remove::<ToolOperationContext>();
    }
}

fn drain_canceled_operations(
    operations: Query<(Entity, &ToolOperationContext, &OperationState)>,
    mut pages: Query<&mut ToolPageState>,
    mut commands: Commands,
) {
    for (entity, operation, state) in &operations {
        if *state != OperationState::Canceled {
            continue;
        }
        if let Ok(mut page) = pages.get_mut(operation.target) {
            page.canceled(operation);
        }
        commands
            .entity(entity)
            .remove::<(ToolOperationContext, PendingToolOperation)>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
