use std::time::{Duration, Instant};

use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_app::{App, Plugin, Update};
use bevy_cef::prelude::UiInput;
use bevy_ecs::prelude::*;
use vmux_ecs::{
    EntityTarget, Operation, OperationPlugin, OperationPolicy, OperationRetained, OperationState,
    OperationTarget, ProfileId, Uuid,
};

use crate::event::{SessionsArchive, SessionsCleanup, SessionsRestoreDeleted, SessionsStageChange};
use crate::{
    ProviderArchiveRequest, ProviderKind, ProviderSessionId, ProviderSessionKey,
    ProviderSessionRestoreRequest, ProviderStageChangeRequest, Session, SessionId, StageId,
};

use super::catalog::{CatalogEntities, CatalogViews};

pub(super) struct ManagerOperationsPlugin;

impl Plugin for ManagerOperationsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.init_resource::<Sequence>()
            .add_observer(stage_local)
            .add_observer(stage_provider)
            .add_observer(archive_local)
            .add_observer(archive_provider)
            .add_observer(cleanup_local)
            .add_observer(cleanup_provider)
            .add_observer(restore)
            .add_systems(
                Update,
                (
                    drain_timers,
                    expire_presentation,
                    wake_presentation,
                    dispatch_stage,
                    dispatch_archive,
                    dispatch_cleanup,
                    dispatch_restore,
                )
                    .chain()
                    .before(crate::mutation::ReceiveSet),
            )
            .add_systems(
                Update,
                (
                    bind_native,
                    finish_native,
                    finish_journal,
                    finish_stage,
                    finish_archive,
                    finish_cleanup,
                    finish_restore,
                )
                    .chain()
                    .after(crate::model::MutationSet),
            );
    }
}

#[derive(Resource, Default)]
struct Sequence(u32);

impl Sequence {
    fn next(&mut self) -> Option<vmux_ecs::Order> {
        self.0 = self.0.checked_add(1)?;
        Some(vmux_ecs::Order(self.0))
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct StageReads<'w, 's> {
    current: Query<'w, 's, &'static crate::Stage, With<Session>>,
    operations: Query<
        'w,
        's,
        (
            &'static OperationTarget,
            &'static StageOperation,
            &'static vmux_ecs::Order,
            &'static OperationState,
        ),
    >,
}

impl StageReads<'_, '_> {
    fn requested(&self, target: Entity) -> Option<&StageId> {
        let mut latest = None;
        for (owner, operation, order, state) in &self.operations {
            if owner.entity() != target
                || !matches!(state, OperationState::Pending | OperationState::Running)
            {
                continue;
            }
            if latest.is_none_or(|(previous, _)| order.0 > previous) {
                latest = Some((order.0, &operation.stage));
            }
        }
        latest.map(|(_, stage)| stage)
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct ArchiveReads<'w, 's> {
    operations: Query<
        'w,
        's,
        (
            &'static OperationTarget,
            &'static ArchiveOperation,
            &'static vmux_ecs::Order,
            &'static OperationState,
        ),
    >,
}

impl ArchiveReads<'_, '_> {
    fn requested(&self, target: Entity) -> Option<bool> {
        let mut latest = None;
        for (owner, operation, order, state) in &self.operations {
            if owner.entity() != target
                || !matches!(state, OperationState::Pending | OperationState::Running)
            {
                continue;
            }
            if latest.is_none_or(|(previous, _)| order.0 > previous) {
                latest = Some((order.0, operation.archived));
            }
        }
        latest.map(|(_, archived)| archived)
    }
}

#[derive(bevy_ecs::system::SystemParam)]
pub(super) struct MutationViews<'w, 's> {
    stages: StageReads<'w, 's>,
    archives: Query<
        'w,
        's,
        (
            &'static OperationTarget,
            &'static ArchiveOperation,
            &'static vmux_ecs::Order,
            &'static OperationState,
            &'static PresentationUntil,
        ),
    >,
    cleanups: Query<
        'w,
        's,
        (
            &'static OperationTarget,
            &'static CleanupOperation,
            &'static OperationState,
        ),
    >,
}

impl MutationViews<'_, '_> {
    pub(super) fn stage(&self, target: Entity) -> Option<&StageId> {
        self.stages.requested(target)
    }
    pub(super) fn archive(&self, target: Entity) -> Option<&ArchiveOperation> {
        let mut latest = None;
        for (owner, operation, order, state, _) in &self.archives {
            if owner.entity() != target
                || !matches!(state, OperationState::Pending | OperationState::Running)
            {
                continue;
            }
            if latest.is_none_or(|(previous, _)| order.0 > previous) {
                latest = Some((order.0, operation));
            }
        }
        latest.map(|(_, operation)| operation)
    }
    pub(super) fn leaving(&self, target: Entity, view: Entity) -> bool {
        for (owner, operation, _, state, deadline) in &self.archives {
            if owner.entity() == target
                && operation.requester == view
                && deadline.0 > Instant::now()
                && matches!(state, OperationState::Pending | OperationState::Running)
            {
                return true;
            }
        }
        for (owner, operation, state) in &self.cleanups {
            if owner.entity() == target
                && operation.requester == view
                && matches!(state, OperationState::Pending | OperationState::Running)
            {
                return true;
            }
        }
        false
    }
}

#[derive(Component)]
pub(super) struct StageOperation {
    pub stage: StageId,
    source_updated_at: i64,
}

#[derive(Component)]
pub(super) struct ArchiveOperation {
    pub requester: Entity,
    pub archived: bool,
    pub accepted_at: i64,
    native: bool,
}

#[derive(Component)]
pub(super) struct CleanupOperation {
    pub requester: Entity,
}

#[derive(Component)]
struct RestoreOperation;

#[derive(Component)]
struct Dispatched;

#[derive(Component)]
struct JournalCommitted;

#[derive(Component)]
struct NativeAttempt(u64);

#[derive(Component)]
struct NativeCompleted(bool);

#[derive(Component)]
pub(super) struct PresentationUntil(pub(super) Instant);

#[derive(Component)]
struct PresentationExpired;

#[derive(Component)]
struct DelayTask(Task<()>);

impl DelayTask {
    fn spawn(
        duration: Duration,
        proxy: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        Self(
            IoTaskPool::get_or_init(bevy::tasks::TaskPool::new).spawn(async move {
                std::thread::sleep(duration);
                if let Some(proxy) = proxy {
                    let _ = proxy.send_event(WinitUserEvent::WakeUp);
                }
            }),
        )
    }
}

fn drain_timers(mut timers: Query<(Entity, &mut DelayTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut timers {
        if future::block_on(future::poll_once(&mut task.0)).is_some() {
            commands.entity(entity).despawn();
        }
    }
}

fn expire_presentation(
    operations: Query<
        (Entity, &PresentationUntil, &ArchiveOperation),
        Without<PresentationExpired>,
    >,
    mut commands: Commands,
) {
    let now = Instant::now();
    let mut views = std::collections::HashSet::new();
    for (entity, deadline, operation) in &operations {
        if deadline.0 > now {
            continue;
        }
        commands.entity(entity).insert(PresentationExpired);
        views.insert(operation.requester);
    }
    for entity in views {
        commands.trigger(super::manager::Project { entity });
    }
}

fn wake_presentation(
    operations: Query<&PresentationUntil, Without<PresentationExpired>>,
    timers: Query<(), With<DelayTask>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !timers.is_empty() {
        return;
    }
    let now = Instant::now();
    let mut next = None;
    for deadline in &operations {
        if deadline.0 > now && next.is_none_or(|previous| deadline.0 < previous) {
            next = Some(deadline.0);
        }
    }
    let Some(deadline) = next else {
        return;
    };
    commands.spawn(DelayTask::spawn(
        deadline.saturating_duration_since(now),
        proxy.as_deref().map(|proxy| (**proxy).clone()),
    ));
}

fn stage_local(
    event: On<UiInput<SessionsStageChange>>,
    views: CatalogViews,
    entities: CatalogEntities,
    pending: StageReads,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
) {
    let Some((profile_entity, profile_id, context)) = views.context() else {
        return;
    };
    if entities.stage(&event.payload.stage).is_none() {
        return;
    }
    let Some(local) = entities.local(&event.payload.id) else {
        return;
    };
    if !views.can_mutate(event.webview, Some(local.id), &local.cwd.0) {
        return;
    }
    let current = pending
        .requested(local.entity)
        .or_else(|| pending.current.get(local.entity).ok().map(|stage| &stage.0));
    if current.is_some_and(|stage| stage.0 == event.payload.stage) {
        return;
    }
    let Some(order) = sequence.next() else {
        return;
    };
    commands.spawn((
        StageOperation {
            stage: StageId(event.payload.stage.clone()),
            source_updated_at: 0,
        },
        Operation::bundle("session.stage").unwrap(),
        OperationRetained,
        order,
        OperationTarget::new(local.entity),
        local.id.clone(),
        EntityTarget::<vmux_ecs::profile::ActiveProfile>::new(profile_entity),
        profile_id.clone(),
        context.clone(),
    ));
}

fn stage_provider(
    event: On<UiInput<SessionsStageChange>>,
    views: CatalogViews,
    entities: CatalogEntities,
    snapshot: crate::catalog::CatalogSources,
    pending: StageReads,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
) {
    if entities.local(&event.payload.id).is_some() {
        return;
    }
    let Some((profile_entity, profile_id, context)) = views.context() else {
        return;
    };
    if entities.stage(&event.payload.stage).is_none() {
        return;
    }
    let Some(provider) = entities.provider(&event.payload.id) else {
        return;
    };
    if !views.can_mutate(event.webview, None, &provider.cwd.0) {
        return;
    }
    let current = snapshot.snapshot();
    let Some(row) = current
        .sessions
        .iter()
        .find(|row| row.id == event.payload.id)
    else {
        return;
    };
    let stage = pending
        .requested(provider.entity)
        .map(|stage| stage.0.as_str())
        .unwrap_or(&row.stage);
    if stage == event.payload.stage {
        return;
    }
    let Some(order) = sequence.next() else {
        return;
    };
    commands.spawn((
        StageOperation {
            stage: StageId(event.payload.stage.clone()),
            source_updated_at: provider.source_updated_at(),
        },
        Operation::bundle("session.stage").unwrap(),
        OperationPolicy::SupersedePrevious,
        OperationRetained,
        order,
        OperationTarget::new(provider.entity),
        provider.kind.clone(),
        provider.id.clone(),
        EntityTarget::<crate::provider::Source>::new(provider.source),
        EntityTarget::<vmux_ecs::profile::ActiveProfile>::new(profile_entity),
        profile_id.clone(),
        context.clone(),
    ));
}

fn archive_local(
    event: On<UiInput<SessionsArchive>>,
    views: CatalogViews,
    entities: CatalogEntities,
    snapshot: crate::catalog::CatalogSources,
    pending: ArchiveReads,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
) {
    let Some((profile_entity, profile_id, context)) = views.context() else {
        return;
    };
    let current = snapshot.snapshot();
    let mut seen = std::collections::HashSet::new();
    for id in &event.payload.ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(local) = entities.local(id) else {
            continue;
        };
        if !views.can_mutate(event.webview, Some(local.id), &local.cwd.0) {
            continue;
        }
        let Some(row) = current.sessions.iter().find(|row| row.id == *id) else {
            continue;
        };
        if pending
            .requested(local.entity)
            .unwrap_or(row.archived_at > 0)
            == event.payload.archived
        {
            continue;
        }
        let Some(order) = sequence.next() else {
            continue;
        };
        let native = match (local.kind, local.provider_id) {
            (Some(kind), Some(id)) => snapshot.native_archive(kind, id),
            _ => false,
        };
        let delay = Duration::from_millis(200);
        let mut operation = commands.spawn((
            ArchiveOperation {
                requester: event.webview,
                archived: event.payload.archived,
                accepted_at: vmux_ecs::UnixMillis::now().0,
                native,
            },
            Operation::bundle("session.archive").unwrap(),
            OperationRetained,
            order,
            OperationTarget::new(local.entity),
            local.id.clone(),
            EntityTarget::<vmux_ecs::profile::ActiveProfile>::new(profile_entity),
            profile_id.clone(),
            context.clone(),
            PresentationUntil(Instant::now() + delay),
        ));
        if let (Some(kind), Some(id)) = (local.kind, local.provider_id) {
            operation.insert((kind.clone(), id.clone()));
        }
    }
}

fn archive_provider(
    event: On<UiInput<SessionsArchive>>,
    views: CatalogViews,
    entities: CatalogEntities,
    snapshot: crate::catalog::CatalogSources,
    pending: ArchiveReads,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
) {
    let Some((profile_entity, profile_id, context)) = views.context() else {
        return;
    };
    let current = snapshot.snapshot();
    let mut seen = std::collections::HashSet::new();
    for id in &event.payload.ids {
        if !seen.insert(id) || entities.local(id).is_some() {
            continue;
        }
        let Some(provider) = entities.provider(id) else {
            continue;
        };
        if !views.can_mutate(event.webview, None, &provider.cwd.0) {
            continue;
        }
        let Some(row) = current.sessions.iter().find(|row| row.id == *id) else {
            continue;
        };
        if pending
            .requested(provider.entity)
            .unwrap_or(row.archived_at > 0)
            == event.payload.archived
        {
            continue;
        }
        let Some(order) = sequence.next() else {
            continue;
        };
        let delay = Duration::from_millis(200);
        commands.spawn((
            ArchiveOperation {
                requester: event.webview,
                archived: event.payload.archived,
                accepted_at: vmux_ecs::UnixMillis::now().0,
                native: provider.archive_observed_at.is_some(),
            },
            Operation::bundle("session.archive").unwrap(),
            OperationRetained,
            order,
            OperationTarget::new(provider.entity),
            provider.kind.clone(),
            provider.id.clone(),
            EntityTarget::<crate::provider::Source>::new(provider.source),
            EntityTarget::<vmux_ecs::profile::ActiveProfile>::new(profile_entity),
            profile_id.clone(),
            context.clone(),
            PresentationUntil(Instant::now() + delay),
        ));
    }
}

fn cleanup_local(
    event: On<UiInput<SessionsCleanup>>,
    views: CatalogViews,
    entities: CatalogEntities,
    pending: Query<(&OperationTarget, &OperationState), With<CleanupOperation>>,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
) {
    let Some((profile_entity, profile_id, context)) = views.context() else {
        return;
    };
    let mut seen = std::collections::HashSet::new();
    for id in &event.payload.ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(local) = entities.local(id) else {
            continue;
        };
        if !views.can_mutate(event.webview, Some(local.id), &local.cwd.0)
            || pending.iter().any(|(target, state)| {
                target.entity() == local.entity
                    && matches!(state, OperationState::Pending | OperationState::Running)
            })
        {
            continue;
        }
        let Some(order) = sequence.next() else {
            continue;
        };
        let mut operation = commands.spawn((
            CleanupOperation {
                requester: event.webview,
            },
            Operation::bundle("session.cleanup").unwrap(),
            OperationRetained,
            order,
            OperationTarget::new(local.entity),
            local.id.clone(),
            EntityTarget::<vmux_ecs::profile::ActiveProfile>::new(profile_entity),
            profile_id.clone(),
            context.clone(),
        ));
        if let (Some(kind), Some(id)) = (local.kind, local.provider_id) {
            operation.insert((kind.clone(), id.clone()));
        }
    }
}

fn cleanup_provider(
    event: On<UiInput<SessionsCleanup>>,
    views: CatalogViews,
    entities: CatalogEntities,
    snapshot: crate::catalog::CatalogSources,
    pending: Query<(&OperationTarget, &OperationState), With<CleanupOperation>>,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
) {
    let Some((profile_entity, profile_id, context)) = views.context() else {
        return;
    };
    let current = snapshot.snapshot();
    let mut seen = std::collections::HashSet::new();
    for id in &event.payload.ids {
        if !seen.insert(id) || entities.local(id).is_some() {
            continue;
        }
        let Some(provider) = entities.provider(id) else {
            continue;
        };
        if !current.sessions.iter().any(|row| row.id == *id)
            || !views.can_mutate(event.webview, None, &provider.cwd.0)
            || pending.iter().any(|(target, state)| {
                target.entity() == provider.entity
                    && matches!(state, OperationState::Pending | OperationState::Running)
            })
        {
            continue;
        }
        let Some(order) = sequence.next() else {
            continue;
        };
        commands.spawn((
            CleanupOperation {
                requester: event.webview,
            },
            Operation::bundle("session.cleanup").unwrap(),
            OperationRetained,
            order,
            OperationTarget::new(provider.entity),
            provider.kind.clone(),
            provider.id.clone(),
            EntityTarget::<crate::provider::Source>::new(provider.source),
            EntityTarget::<vmux_ecs::profile::ActiveProfile>::new(profile_entity),
            profile_id.clone(),
            context.clone(),
        ));
    }
}

fn restore(
    event: On<UiInput<SessionsRestoreDeleted>>,
    views: CatalogViews,
    tombstones: Query<
        (Entity, &ProviderKind, &ProviderSessionId),
        With<crate::ProviderSessionTombstone>,
    >,
    pending: Query<&EntityTarget<crate::ProviderSessionTombstone>, With<RestoreOperation>>,
    mut commands: Commands,
) {
    if !views.manager_contains(event.webview)
        || !views.can_mutate(event.webview, None, std::path::Path::new(""))
    {
        return;
    }
    let Some((profile_entity, profile_id, context)) = views.context() else {
        return;
    };
    for (entity, kind, id) in &tombstones {
        if !event
            .payload
            .sessions
            .iter()
            .any(|key| key.kind == kind.0 && key.sid == id.0)
            || pending.iter().any(|target| target.entity() == entity)
        {
            continue;
        }
        commands.spawn((
            RestoreOperation,
            Operation::bundle("session.restore").unwrap(),
            OperationRetained,
            EntityTarget::<crate::ProviderSessionTombstone>::new(entity),
            kind.clone(),
            id.clone(),
            EntityTarget::<vmux_ecs::profile::ActiveProfile>::new(profile_entity),
            profile_id.clone(),
            context.clone(),
        ));
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct Accepted<'w, 's> {
    profile: vmux_ecs::profile::CurrentProfile<'w, 's>,
    contexts: Query<'w, 's, &'static crate::provider::NativeContext>,
    sessions: Query<'w, 's, &'static SessionId, With<Session>>,
    providers: crate::provider::Entries<'w, 's>,
}

impl Accepted<'_, '_> {
    fn profile(
        &self,
        entity: Entity,
        target: &EntityTarget<vmux_ecs::profile::ActiveProfile>,
        id: &ProfileId,
    ) -> bool {
        self.contexts.get(entity).is_ok_and(|context| {
            context.matches(
                self.profile.entity(),
                self.profile.profile(),
                self.profile.paths(),
            )
        }) && self.profile.entity() == Some(target.entity())
            && self
                .profile
                .profile()
                .is_some_and(|profile| profile.id() == id.0)
    }
    fn session(&self, target: &OperationTarget, id: &SessionId) -> bool {
        self.sessions.get(target.entity()) == Ok(id)
    }
    fn provider(
        &self,
        target: &OperationTarget,
        source: &EntityTarget<crate::provider::Source>,
        kind: &ProviderKind,
        id: &ProviderSessionId,
    ) -> bool {
        self.providers
            .record(target.entity())
            .is_some_and(|record| {
                record.source == source.entity() && record.kind == kind && record.id == id
            })
    }
}

type PendingStage<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static StageOperation,
        &'static OperationState,
        &'static Uuid,
        &'static EntityTarget<vmux_ecs::profile::ActiveProfile>,
        &'static ProfileId,
        &'static vmux_ecs::Order,
        Option<&'static OperationTarget>,
        Option<&'static SessionId>,
        Option<&'static EntityTarget<crate::provider::Source>>,
        Option<&'static ProviderKind>,
        Option<&'static ProviderSessionId>,
    ),
    Without<Dispatched>,
>;

fn dispatch_stage(
    operations: PendingStage,
    accepted: Accepted,
    mut local: MessageWriter<crate::mutation::Request>,
    mut providers: MessageWriter<ProviderStageChangeRequest>,
    mut commands: Commands,
) {
    let mut ordered = operations.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|(entity, _, _, _, _, _, order, ..)| (order.0, *entity));
    for (
        entity,
        operation,
        state,
        uuid,
        profile,
        profile_id,
        _order,
        target,
        id,
        source,
        kind,
        provider_id,
    ) in ordered
    {
        if *state != OperationState::Pending {
            commands
                .entity(entity)
                .insert(Dispatched)
                .remove::<OperationRetained>();
            continue;
        }
        let Some(target) = target else {
            commands
                .entity(entity)
                .insert((Dispatched, OperationState::Canceled))
                .remove::<OperationRetained>();
            continue;
        };
        if !accepted.profile(entity, profile, profile_id) {
            commands
                .entity(entity)
                .insert((Dispatched, OperationState::Canceled))
                .remove::<OperationRetained>();
            continue;
        }
        if let Some(id) = id {
            if !accepted.session(target, id) {
                commands
                    .entity(entity)
                    .insert((Dispatched, OperationState::Canceled))
                    .remove::<OperationRetained>();
                continue;
            }
            let mut request = crate::mutation::Request::local(
                id.clone(),
                crate::mutation::Mutation::change_stage(operation.stage.clone()),
            );
            request.operation.clone_from(&uuid.0);
            local.write(request);
        } else if let (Some(source), Some(kind), Some(id)) = (source, kind, provider_id) {
            if !accepted.provider(target, source, kind, id) {
                commands
                    .entity(entity)
                    .insert((Dispatched, OperationState::Canceled))
                    .remove::<OperationRetained>();
                continue;
            }
            providers.write(ProviderStageChangeRequest {
                kind: kind.clone(),
                id: id.clone(),
                stage: operation.stage.clone(),
                source_updated_at: operation.source_updated_at,
            });
        } else {
            continue;
        }
        commands
            .entity(entity)
            .insert((Dispatched, OperationState::Running));
    }
}

type PendingArchive<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ArchiveOperation,
        &'static Uuid,
        &'static EntityTarget<vmux_ecs::profile::ActiveProfile>,
        &'static ProfileId,
        &'static vmux_ecs::Order,
        Option<&'static OperationTarget>,
        Option<&'static SessionId>,
        Option<&'static EntityTarget<crate::provider::Source>>,
        Option<&'static ProviderKind>,
        Option<&'static ProviderSessionId>,
    ),
    Without<Dispatched>,
>;

fn dispatch_archive(
    operations: PendingArchive,
    accepted: Accepted,
    mut local: MessageWriter<crate::mutation::Request>,
    mut providers: MessageWriter<ProviderArchiveRequest>,
    mut commands: Commands,
) {
    let mut ordered = operations.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|(entity, _, _, _, _, order, ..)| (order.0, *entity));
    for (
        entity,
        operation,
        uuid,
        profile,
        profile_id,
        _order,
        target,
        id,
        source,
        kind,
        provider_id,
    ) in ordered
    {
        let Some(target) = target else {
            commands
                .entity(entity)
                .insert((Dispatched, OperationState::Canceled));
            continue;
        };
        if !accepted.profile(entity, profile, profile_id) {
            commands
                .entity(entity)
                .insert((Dispatched, OperationState::Canceled));
            continue;
        }
        if let Some(id) = id {
            if !accepted.session(target, id) {
                commands
                    .entity(entity)
                    .insert((Dispatched, OperationState::Canceled));
                continue;
            }
            let mut request = crate::mutation::Request::local(
                id.clone(),
                crate::mutation::Mutation::archive(
                    operation.archived.then_some(operation.accepted_at),
                ),
            );
            request.operation.clone_from(&uuid.0);
            local.write(request);
        } else if let (Some(source), Some(kind), Some(id)) = (source, kind, provider_id) {
            if !accepted.provider(target, source, kind, id) {
                commands
                    .entity(entity)
                    .insert((Dispatched, OperationState::Canceled));
                continue;
            }
        } else {
            continue;
        }
        if let (Some(kind), Some(id)) = (kind, provider_id) {
            providers.write(ProviderArchiveRequest {
                kind: kind.clone(),
                id: id.clone(),
                archived: operation.archived,
                source_updated_at: operation.accepted_at,
                native: operation.native,
            });
        }
        commands
            .entity(entity)
            .insert((Dispatched, OperationState::Running));
    }
}

fn dispatch_cleanup(
    operations: Query<
        (
            Entity,
            Option<&OperationTarget>,
            Option<&EntityTarget<crate::provider::Source>>,
            Option<&SessionId>,
            Option<&ProviderKind>,
            Option<&ProviderSessionId>,
            &EntityTarget<vmux_ecs::profile::ActiveProfile>,
            &ProfileId,
        ),
        (With<CleanupOperation>, Without<Dispatched>),
    >,
    accepted: Accepted,
    mut requests: MessageWriter<crate::CleanupRequest>,
    mut commands: Commands,
) {
    for (entity, target, source, session, kind, id, profile, profile_id) in &operations {
        let current = if target.is_none() {
            false
        } else if let (Some(target), Some(session)) = (target, session) {
            accepted.session(target, session)
        } else if let (Some(target), Some(source), Some(kind), Some(id)) =
            (target, source, kind, id)
        {
            accepted.provider(target, source, kind, id)
        } else {
            false
        };
        if !current || !accepted.profile(entity, profile, profile_id) {
            commands
                .entity(entity)
                .insert((Dispatched, OperationState::Canceled))
                .remove::<OperationRetained>();
            continue;
        }
        let request = if let Some(session) = session {
            crate::CleanupRequest {
                sessions: vec![session.clone()],
                providers: Vec::new(),
            }
        } else if let (Some(kind), Some(id)) = (kind, id) {
            crate::CleanupRequest {
                sessions: Vec::new(),
                providers: vec![ProviderSessionKey::new(kind.clone(), id.clone())],
            }
        } else {
            continue;
        };
        requests.write(request);
        commands
            .entity(entity)
            .insert((Dispatched, OperationState::Running));
    }
}

fn dispatch_restore(
    operations: Query<
        (
            Entity,
            &EntityTarget<crate::ProviderSessionTombstone>,
            &ProviderKind,
            &ProviderSessionId,
            &EntityTarget<vmux_ecs::profile::ActiveProfile>,
            &ProfileId,
        ),
        (With<RestoreOperation>, Without<Dispatched>),
    >,
    accepted: Accepted,
    tombstones: Query<(&ProviderKind, &ProviderSessionId), With<crate::ProviderSessionTombstone>>,
    mut requests: MessageWriter<ProviderSessionRestoreRequest>,
    mut commands: Commands,
) {
    for (entity, target, kind, id, profile, profile_id) in &operations {
        if tombstones.get(target.entity()) != Ok((kind, id))
            || !accepted.profile(entity, profile, profile_id)
        {
            commands
                .entity(entity)
                .insert((Dispatched, OperationState::Canceled))
                .remove::<OperationRetained>();
            continue;
        }
        requests.write(ProviderSessionRestoreRequest {
            keys: vec![ProviderSessionKey::new(kind.clone(), id.clone())],
        });
        commands
            .entity(entity)
            .insert((Dispatched, OperationState::Running));
    }
}

fn finish_journal(
    mut commits: MessageReader<crate::mutation::Committed>,
    mut rejected: MessageReader<crate::mutation::Rejected>,
    operations: Query<(Entity, &Uuid), Or<(With<StageOperation>, With<ArchiveOperation>)>>,
    mut commands: Commands,
) {
    for commit in commits.read() {
        for (entity, id) in &operations {
            if id.0 == commit.operation {
                commands.entity(entity).insert(JournalCommitted);
            }
        }
    }
    for rejection in rejected.read() {
        for (entity, id) in &operations {
            if id.0 == rejection.operation {
                commands.entity(entity).insert(OperationState::Failed);
            }
        }
    }
}

fn bind_native(
    operations: Query<
        (
            Entity,
            &ArchiveOperation,
            &ProviderKind,
            &ProviderSessionId,
            &OperationState,
        ),
        (With<Dispatched>, Without<NativeAttempt>),
    >,
    writes: crate::model::ArchiveWrites,
    mut commands: Commands,
) {
    for (entity, operation, kind, id, state) in &operations {
        if !operation.native || *state != OperationState::Running {
            continue;
        }
        if let Some((revision, archived)) =
            writes.revision(&ProviderSessionKey::new(kind.clone(), id.clone()))
            && archived == operation.archived
        {
            commands.entity(entity).insert(NativeAttempt(revision));
        }
    }
}

fn finish_native(
    mut results: MessageReader<crate::ProviderArchiveCompleted>,
    operations: Query<(
        Entity,
        &NativeAttempt,
        &ArchiveOperation,
        &ProviderKind,
        &ProviderSessionId,
    )>,
    mut commands: Commands,
) {
    for result in results.read() {
        for (entity, attempt, operation, kind, id) in &operations {
            if attempt.0 == result.revision
                && kind == &result.kind
                && id == &result.id
                && operation.archived == result.archived
            {
                commands
                    .entity(entity)
                    .insert(NativeCompleted(result.success));
            }
        }
    }
}

fn finish_stage(
    operations: Query<
        (
            Entity,
            &StageOperation,
            &OperationState,
            Has<JournalCommitted>,
            Option<&ProviderKind>,
            Option<&ProviderSessionId>,
        ),
        With<Dispatched>,
    >,
    stages: Query<(&ProviderKind, &ProviderSessionId, &crate::Stage), With<crate::ProviderStage>>,
    mut commands: Commands,
) {
    for (entity, operation, state, committed, kind, id) in &operations {
        if matches!(state, OperationState::Failed | OperationState::Canceled) {
            commands.entity(entity).remove::<OperationRetained>();
            continue;
        }
        if *state != OperationState::Running {
            continue;
        }
        let provider_committed = match (kind, id) {
            (Some(kind), Some(id)) => stages
                .iter()
                .any(|(k, i, stage)| k == kind && i == id && stage.0 == operation.stage),
            _ => false,
        };
        if committed || provider_committed {
            commands
                .entity(entity)
                .insert(OperationState::Succeeded)
                .remove::<OperationRetained>();
        }
    }
}

fn finish_archive(
    operations: Query<
        (
            Entity,
            &ArchiveOperation,
            &OperationState,
            Has<JournalCommitted>,
            Option<&SessionId>,
            Option<&ProviderKind>,
            Option<&ProviderSessionId>,
            Has<PresentationExpired>,
            Has<NativeAttempt>,
            Option<&NativeCompleted>,
        ),
        With<Dispatched>,
    >,
    writes: crate::model::ArchiveWrites,
    mut commands: Commands,
) {
    for (entity, operation, state, committed, local, kind, id, expired, attempt, outcome) in
        &operations
    {
        if !expired {
            continue;
        }
        if operation.native && attempt && outcome.is_none() {
            continue;
        }
        if matches!(state, OperationState::Canceled | OperationState::Failed) {
            commands.entity(entity).remove::<OperationRetained>();
            continue;
        }
        if *state != OperationState::Running || local.is_some() && !committed {
            continue;
        }
        if operation.native {
            if let Some(outcome) = outcome {
                commands
                    .entity(entity)
                    .insert(if outcome.0 {
                        OperationState::Succeeded
                    } else {
                        OperationState::Failed
                    })
                    .remove::<OperationRetained>();
                continue;
            }
            if !attempt {
                commands
                    .entity(entity)
                    .insert(OperationState::Canceled)
                    .remove::<OperationRetained>();
                continue;
            }
        }
        if let (Some(kind), Some(id)) = (kind, id)
            && writes
                .archived_at(&ProviderSessionKey::new(kind.clone(), id.clone()))
                .is_some()
        {
            continue;
        }
        commands
            .entity(entity)
            .insert(OperationState::Succeeded)
            .remove::<OperationRetained>();
    }
}

fn finish_cleanup(
    operations: Query<
        (
            Entity,
            &OperationState,
            Option<&OperationTarget>,
            Option<&ProviderKind>,
            Option<&ProviderSessionId>,
        ),
        (With<CleanupOperation>, With<Dispatched>),
    >,
    sessions: Query<(), With<Session>>,
    tombstones: Res<crate::ProviderSessionTombstones>,
    mut commands: Commands,
) {
    for (entity, state, session, kind, id) in &operations {
        if *state != OperationState::Running {
            continue;
        }
        if session.is_some_and(|target| sessions.contains(target.entity())) {
            continue;
        }
        if let (Some(kind), Some(id)) = (kind, id)
            && !tombstones.contains(&kind.0, &id.0)
        {
            continue;
        }
        commands
            .entity(entity)
            .insert(OperationState::Succeeded)
            .remove::<OperationRetained>();
    }
}

fn finish_restore(
    operations: Query<
        (
            Entity,
            &OperationState,
            &EntityTarget<crate::ProviderSessionTombstone>,
        ),
        (With<RestoreOperation>, With<Dispatched>),
    >,
    tombstones: Query<(), With<crate::ProviderSessionTombstone>>,
    mut commands: Commands,
) {
    for (entity, state, target) in &operations {
        if *state == OperationState::Running && !tombstones.contains(target.entity()) {
            commands
                .entity(entity)
                .insert(OperationState::Succeeded)
                .remove::<OperationRetained>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::manager_fixture::ManagerFixture;
    use super::*;

    #[test]
    fn provider_stage_reversals_finish_superseded_work_and_keep_the_latest_selection() {
        let mut fixture = ManagerFixture::new();
        fixture.provider("provider", "Provider task");
        fixture.update();
        for stage in ["done", "in_review", "in_progress"] {
            fixture.input(SessionsStageChange {
                id: "acp:codex:provider".into(),
                stage: stage.into(),
                ..Default::default()
            });
        }
        fixture.app.world_mut().entity_mut(fixture.view).despawn();
        fixture.app.update();
        let stages = fixture
            .app
            .world_mut()
            .query_filtered::<&crate::Stage, With<crate::ProviderStage>>()
            .iter(fixture.app.world())
            .map(|stage| stage.0.0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(stages, vec!["in_progress"]);
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, (With<StageOperation>, With<OperationRetained>)>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn failed_native_archive_is_not_success_and_rejects_an_unmatched_receipt_after_page_removal() {
        let mut fixture = ManagerFixture::new();
        let session = fixture.session("session", "Task", vmux_ecs::UnixMillis::now().0);
        fixture.app.world_mut().entity_mut(session).insert((
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider".into()),
        ));
        fixture.provider("provider", "Provider task");
        fixture.update();
        fixture.input(SessionsArchive {
            ids: vec!["session".into()],
            archived: true,
        });
        fixture.app.world_mut().entity_mut(fixture.view).despawn();
        fixture.app.update();
        let operation = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<ArchiveOperation>>()
            .single(fixture.app.world())
            .unwrap();
        let writes = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<crate::ProviderArchiveWriteRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(writes.len(), 1);
        let request = &writes[0];
        fixture
            .app
            .world_mut()
            .write_message(crate::ProviderArchiveCompleted {
                kind: request.kind.clone(),
                id: request.id.clone(),
                archived: true,
                revision: request.revision + 1,
                success: true,
            });
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Running)
        );
        fixture
            .app
            .world_mut()
            .write_message(crate::ProviderArchiveCompleted {
                kind: request.kind.clone(),
                id: request.id.clone(),
                archived: true,
                revision: request.revision,
                success: false,
            });
        let deadline = Instant::now() + Duration::from_secs(2);
        while fixture
            .app
            .world()
            .get::<OperationRetained>(operation)
            .is_some()
            && Instant::now() < deadline
        {
            fixture.app.update();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert!(
            fixture
                .app
                .world()
                .get::<OperationRetained>(operation)
                .is_none()
        );
    }

    #[test]
    fn cleanup_refuses_a_replacement_session_with_the_same_id() {
        let mut fixture = ManagerFixture::new();
        let original = fixture.session("session", "Original", vmux_ecs::UnixMillis::now().0);
        fixture.update();
        fixture.input(SessionsCleanup {
            ids: vec!["session".into()],
        });
        fixture.app.world_mut().entity_mut(original).despawn();
        let replacement = fixture.session("session", "Replacement", vmux_ecs::UnixMillis::now().0);
        fixture.app.update();
        assert!(fixture.app.world().get_entity(replacement).is_ok());
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, (With<CleanupOperation>, With<OperationRetained>)>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn local_stage_reversals_preserve_acceptance_order_and_finish_all_work() {
        let mut fixture = ManagerFixture::new();
        let session = fixture.session("session", "Task", vmux_ecs::UnixMillis::now().0);
        fixture.update();
        for stage in ["done", "in_review", "in_progress"] {
            fixture.input(SessionsStageChange {
                id: "session".into(),
                stage: stage.into(),
                ..Default::default()
            });
        }
        fixture.app.world_mut().entity_mut(fixture.view).despawn();
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<crate::Stage>(session)
                .unwrap()
                .0
                .0,
            "in_progress"
        );
        let retained = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, (With<StageOperation>, With<OperationRetained>)>()
            .iter(fixture.app.world())
            .count();
        assert_eq!(retained, 0);
    }

    #[test]
    fn repeated_stage_intent_commits_once_after_requester_removal() {
        let mut fixture = ManagerFixture::new();
        let session = fixture.session("session", "Task", vmux_ecs::UnixMillis::now().0);
        fixture.update();
        for _ in 0..2 {
            fixture.input(SessionsStageChange {
                id: "session".into(),
                kind: "spoofed".into(),
                sid: "elsewhere".into(),
                stage: "done".into(),
            });
        }
        fixture.app.world_mut().entity_mut(fixture.view).despawn();
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<crate::Stage>(session)
                .unwrap()
                .0
                .0,
            "done"
        );
        let records = fixture
            .app
            .world_mut()
            .query::<(&crate::mutation::Mutation, &ChildOf)>()
            .iter(fixture.app.world())
            .filter(|(mutation, owner)| {
                owner.parent() == session
                    && mutation.kind == crate::mutation::MutationKind::ChangeStage
            })
            .count();
        assert_eq!(records, 1);
    }

    #[test]
    fn unknown_stage_and_replaced_profile_refuse_mutation() {
        let mut fixture = ManagerFixture::new();
        let session = fixture.session("session", "Task", vmux_ecs::UnixMillis::now().0);
        fixture.update();
        fixture.input(SessionsStageChange {
            id: "session".into(),
            stage: "unknown".into(),
            ..Default::default()
        });
        fixture.app.update();
        let profile = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<vmux_ecs::profile::ActiveProfile>>()
            .single(fixture.app.world())
            .unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(profile)
            .insert(vmux_ecs::profile::ActiveProfile(
                vmux_ecs::profile::Profile::named("replacement"),
            ));
        fixture.input(SessionsStageChange {
            id: "session".into(),
            stage: "done".into(),
            ..Default::default()
        });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<crate::Stage>(session)
                .unwrap()
                .0
                .0,
            "in_progress"
        );
    }

    #[test]
    fn delete_and_restore_use_durable_tombstones_after_the_manager_reopens() {
        let mut fixture = ManagerFixture::new();
        let session = fixture.session("session", "Task", vmux_ecs::UnixMillis::now().0);
        fixture.app.world_mut().entity_mut(session).insert((
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider".into()),
        ));
        fixture.update();
        fixture.input(SessionsCleanup {
            ids: vec!["session".into(), "session".into()],
        });
        fixture.app.world_mut().entity_mut(fixture.view).despawn();
        let deadline = Instant::now() + Duration::from_secs(3);
        while fixture.app.world().get_entity(session).is_ok() && Instant::now() < deadline {
            fixture.app.update();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(fixture.app.world().get_entity(session).is_err());
        fixture.view = fixture
            .app
            .world_mut()
            .spawn(super::super::catalog::SessionManagerView)
            .id();
        fixture.app.update();
        let targets = fixture.app.world_mut().query_filtered::<(&ProviderKind,&ProviderSessionId),With<crate::ProviderSessionTombstone>>().iter(fixture.app.world()).map(|(kind,id)|crate::event::ProviderSessionTarget { kind:kind.0.clone(),sid:id.0.clone() }).collect::<Vec<_>>();
        assert_eq!(
            targets,
            vec![crate::event::ProviderSessionTarget {
                kind: "acp:codex".into(),
                sid: "provider".into()
            }]
        );
        fixture.input(SessionsRestoreDeleted { sessions: targets });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<crate::ProviderSessionTombstone>>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }
}
