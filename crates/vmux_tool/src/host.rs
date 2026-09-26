use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_core::host::{UiState, UiStatePlugin, UiStateWrite};
use vmux_core::tool::{
    ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest, ToolInstallRequest,
    ToolLinkRequest, ToolOpenRequest, ToolOperationKey, ToolOperationNotice, ToolUninstallRequest,
    ToolUnlinkRequest, ToolUpdateRequest, ToolsNavigateRequest, ToolsRefreshRequest, ToolsSnapshot,
    ToolsUiState,
};

use crate::{
    ToolOperationFailed, ToolOperationFinished, ToolOperationRequest, ToolOperationSucceeded,
    ToolStore, ToolStoreTarget, ToolsManifest,
};

pub(crate) struct ToolHostPlugin;

impl Plugin for ToolHostPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::McpConnectionPlugin,
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
        .add_observer(on_tool_operation_request::<ToolInstallRequest>)
        .add_observer(on_tool_operation_request::<ToolUpdateRequest>)
        .add_observer(on_tool_operation_request::<ToolUninstallRequest>)
        .add_observer(on_tool_operation_request::<ToolForgetRequest>)
        .add_observer(on_tool_operation_request::<ToolAdoptRequest>)
        .add_observer(on_tool_operation_request::<ToolLinkRequest>)
        .add_observer(on_tool_operation_request::<ToolUnlinkRequest>)
        .add_observer(on_tool_operation_request::<ToolApplyRequest>)
        .add_observer(on_tool_operation_request::<ToolImportRequest>)
        .add_systems(Startup, spawn_tool_registry)
        .add_systems(
            Update,
            (
                request_tool_scan,
                finish_tool_scans,
                start_tool_operation,
                drain_succeeded_tool_operations,
                drain_failed_tool_operations,
                emit_tools_state,
            )
                .chain(),
        );
    }
}

fn spawn_tool_registry(mut commands: Commands) {
    commands.spawn((
        Name::new("Tool UI registry"),
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

#[derive(Component, Clone)]
pub struct ToolScanRequest {
    generation: u64,
    refresh_catalogs: bool,
}

impl ToolScanRequest {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn refresh_catalogs(&self) -> bool {
        self.refresh_catalogs
    }
}

#[derive(Component)]
pub struct ToolScanOutput {
    snapshot: ToolsSnapshot,
    manifest: ToolsManifest,
}

impl ToolScanOutput {
    pub fn new(snapshot: ToolsSnapshot, manifest: ToolsManifest) -> Self {
        Self { snapshot, manifest }
    }
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

fn on_tool_operation_request<R>(
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

fn start_tool_operation(
    pending: Query<(Entity, &ToolOperationContext), With<PendingToolOperation>>,
    active: Query<(), With<ToolStoreTarget>>,
    scans: Query<(), With<ToolScanRequest>>,
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

fn request_tool_scan(
    mut registry: Query<(Entity, &mut ToolRegistry, &ToolStore)>,
    scans: Query<(), With<ToolScanRequest>>,
    tool_operations: Query<(), With<ToolStoreTarget>>,
    pending_tool_operations: Query<(), With<PendingToolOperation>>,
    mut commands: Commands,
) {
    let Ok((store, mut state, _)) = registry.single_mut() else {
        return;
    };
    if !state.dirty
        || !scans.is_empty()
        || !tool_operations.is_empty()
        || !pending_tool_operations.is_empty()
    {
        return;
    }
    commands.spawn((
        Name::new("Tool inventory scan"),
        ToolScanRequest {
            generation: state.generation,
            refresh_catalogs: state.refresh_catalogs,
        },
        ToolStoreTarget(store),
    ));
    state.dirty = false;
    state.refresh_catalogs = false;
}

fn finish_tool_scans(
    scans: Query<(Entity, &ToolScanRequest, &ToolScanOutput), Added<ToolScanOutput>>,
    mut registry: Query<(&mut ToolRegistry, &mut ToolsManifest)>,
    mut commands: Commands,
) {
    let Ok((mut state, mut manifest)) = registry.single_mut() else {
        return;
    };
    for (entity, request, output) in &scans {
        commands.entity(entity).despawn();
        if request.generation != state.generation {
            state.dirty = true;
            continue;
        }
        state.snapshot.clone_from(&output.snapshot);
        manifest.clone_from(&output.manifest);
        state.revision = state.revision.wrapping_add(1);
    }
}

fn drain_succeeded_tool_operations(
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

fn drain_failed_tool_operations(
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
    use vmux_core::tool::{ToolOperationKind, ToolProvider};

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
}
