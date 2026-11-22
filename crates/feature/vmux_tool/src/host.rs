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
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::{EntityTarget, OperationState, PageMetadata, TaskWaker, UiStatePlugin};

use crate::{
    ExternalToolOperation, ToolInventoryChanged, ToolOperationFailed, ToolOperationFinished,
    ToolOperationRequest, ToolOperationSucceeded, ToolProviderId, ToolProviderTarget, ToolStore,
    ToolStoreTarget, ToolTaskSet,
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
        .add_systems(
            Update,
            (
                (
                    refuse_unsupported_operations::<ToolInstallRequest>,
                    refuse_unsupported_operations::<ToolUpdateRequest>,
                    refuse_unsupported_operations::<ToolUninstallRequest>,
                    refuse_unsupported_operations::<ToolForgetRequest>,
                    refuse_unsupported_operations::<ToolAdoptRequest>,
                    refuse_unsupported_operations::<ToolLinkRequest>,
                    refuse_unsupported_operations::<ToolUnlinkRequest>,
                    refuse_unsupported_operations::<ToolApplyRequest>,
                    refuse_unsupported_operations::<ToolImportRequest>,
                )
                    .in_set(ToolTaskSet::Start),
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
    let auth = if payload.oauth {
        crate::McpAuthSpec::OAuth { scopes: Vec::new() }
    } else {
        crate::McpAuthSpec::None
    };
    let spec = match payload.transport.trim().to_ascii_lowercase().as_str() {
        "http" => crate::McpServerSpec::Http {
            url: target_value,
            headers: Default::default(),
            header_env: Default::default(),
            bearer_token_env_var: bearer,
            auth,
            enabled: true,
            agents: Vec::new(),
        },
        "sse" => crate::McpServerSpec::Sse {
            url: target_value,
            headers: Default::default(),
            header_env: Default::default(),
            bearer_token_env_var: bearer,
            auth,
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

fn refuse_unsupported_operations<R: Clone + Send + Sync + 'static>(
    operations: Query<
        (
            Entity,
            &ToolOperationRequest<R>,
            &ToolStoreTarget,
            Option<&OperationState>,
        ),
        (With<ExternalToolOperation>, Without<ToolOperationFinished>),
    >,
    stores: Query<(), With<ToolStore>>,
    mut commands: Commands,
) where
    ToolOperationKey: From<R>,
{
    for (entity, request, target, state) in &operations {
        if !state
            .is_some_and(|state| matches!(state, OperationState::Pending | OperationState::Running))
        {
            continue;
        }
        let operation = ToolOperationKey::from(request.0.clone());
        let message = if stores.contains(target.entity()) {
            format!(
                "{} does not support this operation",
                operation.provider.id()
            )
        } else {
            "tool store entity is unavailable".to_string()
        };
        commands
            .entity(entity)
            .remove::<ExternalToolOperation>()
            .insert((ToolOperationFinished, ToolOperationFailed(message)));
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
    use vmux_ecs::PageOpenRequest;

    #[test]
    fn unclaimed_external_operation_is_refused_without_a_worker() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(directory.path(), directory.path());
        let mut app = App::new();
        app.add_plugins((crate::ToolRuntimePlugin, ToolHostPlugin))
            .add_message::<PageOpenRequest>();
        let store_entity = app.world_mut().spawn(store.clone()).id();
        let operation = app
            .world_mut()
            .spawn((
                ToolStoreTarget::new(store_entity),
                ToolOperationRequest(ToolInstallRequest {
                    provider: ToolProvider::new("custom"),
                    id: "pkg".into(),
                }),
            ))
            .id();

        app.update();
        app.update();

        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert_eq!(
            app.world()
                .get::<ToolOperationFailed>(operation)
                .map(|failure| failure.0.as_str()),
            Some("custom does not support this operation")
        );
        assert!(
            app.world()
                .get::<ExternalToolOperation>(operation)
                .is_none()
        );
        assert!(store.lease().is_some());
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
