use std::collections::HashMap;
use std::time::Duration;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};
use serde::{Deserialize, Serialize};
use vmux_time::{
    BackoffPolicy, Clock, Deadline, DeadlinePlugin, DeadlineReached, RetryBackoff, TimePlugin,
};
use web_time::Instant;

use crate::Uuid;

pub struct OperationPlugin;

impl Plugin for OperationPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimePlugin>() {
            app.add_plugins(TimePlugin);
        }
        if !app.is_plugin_added::<DeadlinePlugin<TimeoutDeadline>>() {
            app.add_plugins(DeadlinePlugin::<TimeoutDeadline>::default());
        }
        if !app.is_plugin_added::<DeadlinePlugin<RetryDeadline>>() {
            app.add_plugins(DeadlinePlugin::<RetryDeadline>::default());
        }
        app.register_type::<Operation>()
            .register_type::<OperationKind>()
            .register_type::<OperationState>()
            .register_type::<OperationPolicy>()
            .register_type::<OperationTargetRevision>()
            .register_type::<OperationError>()
            .init_resource::<OperationExpiry>()
            .init_resource::<Sequence>()
            .configure_sets(Update, (OperationSet::Policy, OperationSet::Admit).chain())
            .configure_sets(PostUpdate, OperationSet::Finish)
            .add_observer(complete)
            .add_observer(stamp_policy)
            .add_observer(attach_target)
            .add_observer(detach_target)
            .add_observer(arm_timeout)
            .add_observer(clear_timeout)
            .add_observer(on_timeout)
            .add_observer(retry_requested)
            .add_observer(on_retry_due)
            .add_systems(Update, enforce_supersede.in_set(OperationSet::Policy))
            .add_systems(Update, admit.in_set(OperationSet::Admit))
            .add_systems(
                PostUpdate,
                (finish, admit, cleanup)
                    .chain()
                    .in_set(OperationSet::Finish),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperationSet {
    Policy,
    Admit,
    Finish,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Operation;

impl Operation {
    pub fn bundle(kind: impl Into<String>) -> Result<OperationBundle, String> {
        Ok(OperationBundle {
            operation: Self,
            id: Uuid(uuid::Uuid::new_v4().to_string()),
            kind: OperationKind::new(kind)?,
            state: OperationState::Pending,
        })
    }
}

#[derive(Bundle)]
pub struct OperationBundle {
    operation: Operation,
    id: Uuid,
    kind: OperationKind,
    state: OperationState,
}

impl OperationBundle {
    pub fn running(mut self) -> Self {
        self.state = OperationState::Running;
        self
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub enum OperationPolicy {
    SerializePerTarget,
    SupersedePrevious,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationAdmitted;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationTimeout {
    duration: Duration,
}

impl OperationTimeout {
    pub const fn new(duration: Duration) -> Self {
        Self { duration }
    }

    pub const fn from_secs(secs: u64) -> Self {
        Self::new(Duration::from_secs(secs))
    }

    pub const fn from_millis(millis: u64) -> Self {
        Self::new(Duration::from_millis(millis))
    }

    pub const fn duration(&self) -> Duration {
        self.duration
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationRetry {
    policy: BackoffPolicy,
    attempts: u32,
}

impl OperationRetry {
    pub const fn new(policy: BackoffPolicy) -> Self {
        Self {
            policy,
            attempts: 0,
        }
    }

    pub const fn exponential(initial: Duration, max: Duration) -> Self {
        Self::new(BackoffPolicy::exponential(initial, max))
    }

    pub const fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.policy = self.policy.with_max_attempts(max_attempts);
        self
    }

    pub const fn policy(&self) -> BackoffPolicy {
        self.policy
    }

    pub const fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn exhausted(&self) -> bool {
        self.policy
            .max_attempts
            .is_some_and(|limit| self.attempts >= limit)
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationRetryWaiting;

#[derive(EntityEvent, Clone, Debug)]
pub struct OperationRetryRequested {
    #[event_target]
    pub entity: Entity,
    pub error: String,
}

impl OperationRetryRequested {
    pub fn new(entity: Entity, error: impl Into<String>) -> Self {
        Self {
            entity,
            error: error.into(),
        }
    }
}

#[derive(EntityEvent)]
pub struct OperationCompleted {
    #[event_target]
    pub entity: Entity,
    pub result: Result<(), String>,
}

impl OperationCompleted {
    pub fn from_result<T>(entity: Entity, result: &Result<T, String>) -> Self {
        let result = match result {
            Ok(_) => Ok(()),
            Err(error) => Err(error.clone()),
        };
        Self { entity, result }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct OperationKind(String);

impl OperationKind {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("operation kind must not be empty".to_string());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub enum OperationState {
    #[default]
    Pending,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl OperationState {
    fn is_live(&self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = TargetedOperations)]
pub struct OperationTarget(Entity);

impl OperationTarget {
    pub const fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub const fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Debug)]
#[relationship_target(relationship = OperationTarget)]
pub struct TargetedOperations(Vec<Entity>);

#[derive(
    Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, serde::Deserialize, serde::Serialize,
)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct OperationTargetRevision(pub u64);

impl OperationTargetRevision {
    pub fn accepts(&self, current: Option<u64>) -> bool {
        current == Some(self.0)
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct OperationError(String);

impl OperationError {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("operation error must not be empty".to_string());
        }
        Ok(Self(value))
    }

    pub fn message(&self) -> &str {
        &self.0
    }

    fn from_message(error: &str, fallback: &str) -> Self {
        let message = if error.trim().is_empty() {
            fallback
        } else {
            error
        };
        Self(message.to_string())
    }
}

#[derive(Component)]
pub struct OperationRetained;

struct TimeoutDeadline;

struct RetryDeadline;

#[derive(Resource, Default)]
struct Sequence(u64);

impl Sequence {
    fn next(&mut self) -> Arrival {
        self.0 = self.0.saturating_add(1);
        Arrival(self.0)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Arrival(u64);

#[derive(Component)]
struct OperationExpiresAt(Instant);

#[derive(Resource, Default)]
struct OperationExpiry(Option<Instant>);

impl OperationExpiry {
    const RETENTION: Duration = Duration::from_secs(300);
}

type SupersedeEntries<'w, 's> = Query<
    'w,
    's,
    (
        &'static OperationKind,
        &'static OperationState,
        Option<&'static OperationPolicy>,
        Option<&'static OperationTarget>,
        Option<&'static Arrival>,
    ),
    With<Operation>,
>;

#[derive(SystemParam)]
struct Supersede<'w, 's> {
    targets: Query<'w, 's, &'static TargetedOperations>,
    entries: SupersedeEntries<'w, 's>,
}

impl Supersede<'_, '_> {
    fn cancel_previous(&self, current: Entity, commands: &mut Commands) {
        let Ok((kind, state, Some(OperationPolicy::SupersedePrevious), Some(target), _)) =
            self.entries.get(current)
        else {
            return;
        };
        if !state.is_live() {
            return;
        }
        let Ok(members) = self.targets.get(target.entity()) else {
            return;
        };
        for other in members.iter() {
            if other == current {
                continue;
            }
            let Ok((other_kind, other_state, ..)) = self.entries.get(other) else {
                continue;
            };
            if other_kind == kind && other_state.is_live() {
                commands
                    .entity(other)
                    .try_insert(OperationState::Canceled)
                    .try_remove::<(
                        OperationAdmitted,
                        OperationRetryWaiting,
                        Deadline<TimeoutDeadline>,
                        RetryBackoff<RetryDeadline>,
                    )>();
            }
        }
    }
}

fn stamp_policy(
    trigger: On<Insert, OperationPolicy>,
    supersede: Supersede,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if let Ok((_, _, _, _, arrival)) = supersede.entries.get(entity)
        && arrival.is_none()
    {
        commands.entity(entity).try_insert(sequence.next());
    }
    supersede.cancel_previous(entity, &mut commands);
}

fn attach_target(
    trigger: On<Insert, OperationTarget>,
    supersede: Supersede,
    mut commands: Commands,
) {
    supersede.cancel_previous(trigger.event_target(), &mut commands);
}

fn detach_target(
    trigger: On<Remove, OperationTarget>,
    mut operations: Query<&mut OperationState, (With<Operation>, With<OperationPolicy>)>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(mut state) = operations.get_mut(entity) else {
        return;
    };
    if state.is_live() {
        *state = OperationState::Canceled;
    }
    commands.entity(entity).try_remove::<(
        OperationAdmitted,
        OperationRetryWaiting,
        Deadline<TimeoutDeadline>,
        RetryBackoff<RetryDeadline>,
    )>();
}

type PolicyEntries<'w, 's> = Query<
    'w,
    's,
    (
        &'static OperationKind,
        &'static OperationState,
        Option<&'static OperationPolicy>,
        Option<&'static Arrival>,
    ),
    With<Operation>,
>;

fn enforce_supersede(
    targets: Query<&TargetedOperations>,
    entries: PolicyEntries,
    mut commands: Commands,
) {
    for members in &targets {
        let mut latest = HashMap::<&str, (u64, Entity)>::new();
        for entity in members.iter() {
            let Ok((kind, state, policy, arrival)) = entries.get(entity) else {
                continue;
            };
            if policy == Some(&OperationPolicy::SupersedePrevious) && state.is_live() {
                let rank = (arrival.map_or(0, |value| value.0), entity);
                latest
                    .entry(kind.as_str())
                    .and_modify(|current| {
                        if rank > *current {
                            *current = rank;
                        }
                    })
                    .or_insert(rank);
            }
        }
        if latest.is_empty() {
            continue;
        }
        for entity in members.iter() {
            let Ok((kind, state, policy, _)) = entries.get(entity) else {
                continue;
            };
            if policy != Some(&OperationPolicy::SupersedePrevious) || !state.is_live() {
                continue;
            }
            if let Some(&(_, winner)) = latest.get(kind.as_str())
                && entity != winner
            {
                commands
                    .entity(entity)
                    .try_insert(OperationState::Canceled)
                    .try_remove::<(
                        OperationAdmitted,
                        OperationRetryWaiting,
                        Deadline<TimeoutDeadline>,
                        RetryBackoff<RetryDeadline>,
                    )>();
            }
        }
    }
}

type SerializedEntries<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static OperationPolicy>,
        &'static mut OperationState,
        Option<&'static Arrival>,
        Has<OperationAdmitted>,
        Has<OperationRetryWaiting>,
    ),
    With<Operation>,
>;

fn admit(
    targets: Query<&TargetedOperations>,
    mut entries: SerializedEntries,
    mut commands: Commands,
) {
    for (entity, _, state, _, has_admitted, waiting) in &entries {
        if has_admitted && (*state != OperationState::Running || waiting) {
            commands.entity(entity).try_remove::<OperationAdmitted>();
        }
    }
    for members in &targets {
        let mut running = false;
        let mut next = None::<(u64, Entity)>;
        for entity in members.iter() {
            let Ok((_, policy, state, arrival, has_admitted, waiting)) = entries.get(entity) else {
                continue;
            };
            if policy != Some(&OperationPolicy::SerializePerTarget) {
                continue;
            }
            if *state == OperationState::Running && !waiting {
                running = true;
                if !has_admitted {
                    commands.entity(entity).try_insert(OperationAdmitted);
                }
                continue;
            }
            if *state == OperationState::Pending && !waiting {
                let rank = (arrival.map_or(u64::MAX, |value| value.0), entity);
                if next.is_none_or(|current| rank < current) {
                    next = Some(rank);
                }
            }
        }
        if running {
            continue;
        }
        if let Some((_, entity)) = next
            && let Ok((_, _, mut state, _, _, _)) = entries.get_mut(entity)
        {
            *state = OperationState::Running;
            commands.entity(entity).try_insert(OperationAdmitted);
        }
    }
}

fn arm_timeout(
    trigger: On<Insert, OperationTimeout>,
    clock: Res<Clock>,
    operations: Query<
        (
            &OperationTimeout,
            &OperationState,
            Has<OperationRetryWaiting>,
        ),
        With<Operation>,
    >,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok((timeout, state, waiting)) = operations.get(entity) else {
        return;
    };
    if !state.is_live() || waiting {
        commands
            .entity(entity)
            .try_remove::<Deadline<TimeoutDeadline>>();
        return;
    }
    commands
        .entity(entity)
        .try_insert(Deadline::<TimeoutDeadline>::after(&clock, timeout.duration));
}

fn clear_timeout(trigger: On<Remove, OperationTimeout>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .try_remove::<Deadline<TimeoutDeadline>>();
}

fn on_timeout(
    trigger: On<DeadlineReached<TimeoutDeadline>>,
    operations: Query<(&OperationState, Has<OperationRetryWaiting>), With<Operation>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok((state, waiting)) = operations.get(entity) else {
        return;
    };
    if !state.is_live() || waiting {
        return;
    }
    commands.trigger(OperationRetryRequested::new(entity, "Operation timed out"));
}

type RetryEntries<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut OperationState,
        Option<&'static mut OperationRetry>,
        Option<&'static mut RetryBackoff<RetryDeadline>>,
        Has<OperationRetryWaiting>,
    ),
    With<Operation>,
>;

fn retry_requested(
    trigger: On<OperationRetryRequested>,
    clock: Res<Clock>,
    mut operations: RetryEntries,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok((mut state, retry, backoff, waiting)) = operations.get_mut(event.entity) else {
        return;
    };
    if !state.is_live() || waiting {
        return;
    }
    let failure = OperationError::from_message(&event.error, "Operation failed");
    if let Some(mut retry) = retry {
        let mut tracker = match backoff {
            Some(existing) => *existing,
            None => RetryBackoff::<RetryDeadline>::new(retry.policy),
        };
        if tracker.record_failure(&clock).is_some() {
            retry.attempts = tracker.attempts();
            *state = OperationState::Pending;
            commands
                .entity(event.entity)
                .try_insert((OperationRetryWaiting, failure, tracker))
                .try_remove::<(OperationAdmitted, Deadline<TimeoutDeadline>)>();
            return;
        }
    }
    *state = OperationState::Failed;
    commands
        .entity(event.entity)
        .try_insert(failure)
        .try_remove::<(
            OperationAdmitted,
            OperationRetryWaiting,
            Deadline<TimeoutDeadline>,
            RetryBackoff<RetryDeadline>,
        )>();
}

type RetryDueEntries<'w, 's> = Query<
    'w,
    's,
    (
        &'static OperationState,
        Option<&'static OperationTimeout>,
        Has<OperationRetryWaiting>,
    ),
    With<Operation>,
>;

fn on_retry_due(
    trigger: On<DeadlineReached<RetryDeadline>>,
    clock: Res<Clock>,
    operations: RetryDueEntries,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok((state, timeout, waiting)) = operations.get(entity) else {
        return;
    };
    if *state != OperationState::Pending || !waiting {
        return;
    }
    let mut entry = commands.entity(entity);
    entry.try_remove::<(OperationRetryWaiting, OperationError)>();
    if let Some(timeout) = timeout {
        entry.try_insert(Deadline::<TimeoutDeadline>::after(&clock, timeout.duration));
    }
}

fn complete(
    trigger: On<OperationCompleted>,
    mut operations: Query<&mut OperationState, With<Operation>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(mut state) = operations.get_mut(event.entity) else {
        return;
    };
    if !state.is_live() {
        return;
    }
    let mut entity = commands.entity(event.entity);
    entity.try_remove::<(
        OperationAdmitted,
        OperationRetryWaiting,
        Deadline<TimeoutDeadline>,
        RetryBackoff<RetryDeadline>,
    )>();
    match &event.result {
        Ok(()) => {
            *state = OperationState::Succeeded;
            entity.try_remove::<OperationError>();
        }
        Err(error) => {
            *state = OperationState::Failed;
            entity.try_insert(OperationError::from_message(error, "Operation failed"));
        }
    }
}

type ChangedOperations<'w, 's> =
    Query<'w, 's, (Entity, &'static OperationState), (With<Operation>, Changed<OperationState>)>;

fn finish(
    operations: ChangedOperations,
    clock: Res<Clock>,
    mut expiry: ResMut<OperationExpiry>,
    mut commands: Commands,
) {
    for (entity, state) in &operations {
        if state.is_live() {
            commands.entity(entity).try_remove::<OperationExpiresAt>();
        } else {
            let deadline = clock.deadline(OperationExpiry::RETENTION);
            if expiry.0.is_none_or(|current| deadline < current) {
                expiry.0 = Some(deadline);
            }
            commands
                .entity(entity)
                .try_insert(OperationExpiresAt(deadline))
                .try_remove::<(
                    OperationAdmitted,
                    OperationRetryWaiting,
                    Deadline<TimeoutDeadline>,
                    RetryBackoff<RetryDeadline>,
                )>();
        }
    }
}

fn cleanup(
    operations: Query<(Entity, &OperationExpiresAt, Has<OperationRetained>), With<Operation>>,
    clock: Res<Clock>,
    mut expiry: ResMut<OperationExpiry>,
    mut commands: Commands,
) {
    let now = clock.now();
    if expiry.0.is_none_or(|deadline| now < deadline) {
        return;
    }
    let mut next = None::<Instant>;
    for (entity, finished, retained) in &operations {
        if finished.0 <= now && !retained {
            commands.entity(entity).despawn();
        } else if next.is_none_or(|deadline| finished.0 < deadline) {
            next = Some(finished.0);
        }
    }
    expiry.0 = next;
}

#[cfg(test)]
mod tests {
    use vmux_time::DeadlineScheduler;

    use super::*;

    #[test]
    fn retained_operation_survives_expiry_until_its_work_is_released() {
        let mut app = App::new();
        app.add_plugins(OperationPlugin);
        let entity = app
            .world_mut()
            .spawn((Operation::bundle("test.write").unwrap(), OperationRetained))
            .id();
        let child = app.world_mut().spawn(ChildOf(entity)).id();
        app.world_mut()
            .entity_mut(entity)
            .insert(OperationState::Canceled);
        app.update();
        let deadline = Instant::now() - Duration::from_secs(1);
        app.world_mut()
            .entity_mut(entity)
            .insert(OperationExpiresAt(deadline));
        app.world_mut().resource_mut::<OperationExpiry>().0 = Some(deadline);
        app.update();
        assert!(app.world().get_entity(entity).is_ok());
        assert!(app.world().get_entity(child).is_ok());
        app.world_mut()
            .entity_mut(entity)
            .remove::<OperationRetained>();
        app.update();
        assert!(app.world().get_entity(entity).is_err());
        assert!(app.world().get_entity(child).is_err());
    }

    #[test]
    fn completed_work_keeps_its_result_and_late_completion_cannot_revive_cancellation() {
        let mut app = App::new();
        app.add_plugins(OperationPlugin);
        let failed = app
            .world_mut()
            .spawn(Operation::bundle("test.write").unwrap())
            .id();
        let canceled = app
            .world_mut()
            .spawn(Operation::bundle("test.cancel").unwrap())
            .id();
        app.world_mut()
            .entity_mut(canceled)
            .insert(OperationState::Canceled);
        app.world_mut().trigger(OperationCompleted {
            entity: failed,
            result: Err("disk full".into()),
        });
        app.world_mut().trigger(OperationCompleted {
            entity: canceled,
            result: Ok(()),
        });
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(failed),
            Some(&OperationState::Failed)
        );
        assert_eq!(
            app.world().get::<OperationError>(failed).unwrap().message(),
            "disk full"
        );
        assert_eq!(
            app.world().get::<OperationState>(canceled),
            Some(&OperationState::Canceled)
        );
        assert!(app.world().get::<OperationExpiresAt>(failed).is_some());
    }

    #[test]
    fn cleanup_retains_live_work_and_releases_expired_results() {
        let mut app = App::new();
        app.add_plugins(OperationPlugin);
        let target = app.world_mut().spawn_empty().id();
        let completed = app
            .world_mut()
            .spawn((
                Operation,
                OperationState::Succeeded,
                OperationTarget::new(target),
            ))
            .id();
        let running = app
            .world_mut()
            .spawn((Operation, OperationState::Running))
            .id();
        app.update();
        assert!(app.world().get_entity(completed).is_ok());

        let past = Instant::now() - Duration::from_secs(1);
        app.world_mut()
            .entity_mut(completed)
            .insert(OperationExpiresAt(past));
        app.world_mut().resource_mut::<OperationExpiry>().0 = Some(past);
        app.update();

        assert!(app.world().get_entity(completed).is_err());
        assert!(app.world().get_entity(running).is_ok());
        assert!(
            app.world()
                .get::<TargetedOperations>(target)
                .is_none_or(|operations| operations.is_empty())
        );
    }

    #[test]
    fn resumed_work_cannot_be_removed_by_its_previous_expiry() {
        let mut app = App::new();
        app.add_plugins(OperationPlugin);
        let operation = app
            .world_mut()
            .spawn((Operation, OperationState::Failed))
            .id();
        app.update();
        let past = Instant::now() - Duration::from_secs(1);
        app.world_mut()
            .entity_mut(operation)
            .insert((OperationState::Running, OperationExpiresAt(past)));
        app.world_mut().resource_mut::<OperationExpiry>().0 = Some(past);
        app.update();
        assert!(app.world().get_entity(operation).is_ok());
        assert!(app.world().get::<OperationExpiresAt>(operation).is_none());
    }

    #[test]
    fn serialize_per_target_admits_strict_fifo_even_after_despawn_and_runs_targets_independently() {
        let mut app = App::new();
        app.add_plugins(OperationPlugin);
        let target_a = app.world_mut().spawn_empty().id();
        let target_b = app.world_mut().spawn_empty().id();
        let a1 = app
            .world_mut()
            .spawn((
                Operation::bundle("work").unwrap(),
                OperationTarget::new(target_a),
                OperationPolicy::SerializePerTarget,
            ))
            .id();
        let a2 = app
            .world_mut()
            .spawn((
                Operation::bundle("work").unwrap(),
                OperationTarget::new(target_a),
                OperationPolicy::SerializePerTarget,
            ))
            .id();
        let a3 = app
            .world_mut()
            .spawn((
                Operation::bundle("work").unwrap(),
                OperationTarget::new(target_a),
                OperationPolicy::SerializePerTarget,
            ))
            .id();
        let b1 = app
            .world_mut()
            .spawn((
                Operation::bundle("work").unwrap(),
                OperationTarget::new(target_b),
                OperationPolicy::SerializePerTarget,
            ))
            .id();

        app.update();
        assert_eq!(
            app.world().get::<OperationState>(a1),
            Some(&OperationState::Running)
        );
        assert!(app.world().get::<OperationAdmitted>(a1).is_some());
        assert_eq!(
            app.world().get::<OperationState>(a2),
            Some(&OperationState::Pending)
        );
        assert!(app.world().get::<OperationAdmitted>(a2).is_none());
        assert_eq!(
            app.world().get::<OperationState>(a3),
            Some(&OperationState::Pending)
        );
        assert_eq!(
            app.world().get::<OperationState>(b1),
            Some(&OperationState::Running)
        );
        assert!(app.world().get::<OperationAdmitted>(b1).is_some());

        app.world_mut().despawn(a1);
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(a2),
            Some(&OperationState::Running)
        );
        assert!(app.world().get::<OperationAdmitted>(a2).is_some());
        assert_eq!(
            app.world().get::<OperationState>(a3),
            Some(&OperationState::Pending)
        );

        app.world_mut().trigger(OperationCompleted {
            entity: a2,
            result: Ok(()),
        });
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(a2),
            Some(&OperationState::Succeeded)
        );
        assert!(app.world().get::<OperationAdmitted>(a2).is_none());
        assert_eq!(
            app.world().get::<OperationState>(a3),
            Some(&OperationState::Running)
        );
        assert!(app.world().get::<OperationAdmitted>(a3).is_some());
    }

    #[test]
    fn supersede_previous_cancels_older_same_target_and_kind_only() {
        let mut app = App::new();
        app.add_plugins(OperationPlugin);
        let target_a = app.world_mut().spawn_empty().id();
        let target_b = app.world_mut().spawn_empty().id();
        let running = app
            .world_mut()
            .spawn((
                Operation::bundle("sync").unwrap().running(),
                OperationTarget::new(target_a),
                OperationPolicy::SupersedePrevious,
            ))
            .id();
        let other_kind = app
            .world_mut()
            .spawn((
                Operation::bundle("fetch").unwrap().running(),
                OperationTarget::new(target_a),
                OperationPolicy::SupersedePrevious,
            ))
            .id();
        let other_target = app
            .world_mut()
            .spawn((
                Operation::bundle("sync").unwrap().running(),
                OperationTarget::new(target_b),
                OperationPolicy::SupersedePrevious,
            ))
            .id();

        let replacement = app
            .world_mut()
            .spawn((
                Operation::bundle("sync").unwrap(),
                OperationTarget::new(target_a),
                OperationPolicy::SupersedePrevious,
            ))
            .id();

        assert_eq!(
            app.world().get::<OperationState>(running),
            Some(&OperationState::Canceled)
        );
        assert_eq!(
            app.world().get::<OperationState>(replacement),
            Some(&OperationState::Pending)
        );
        assert_eq!(
            app.world().get::<OperationState>(other_kind),
            Some(&OperationState::Running)
        );
        assert_eq!(
            app.world().get::<OperationState>(other_target),
            Some(&OperationState::Running)
        );

        let late_target = app
            .world_mut()
            .spawn((
                Operation::bundle("sync").unwrap(),
                OperationPolicy::SupersedePrevious,
            ))
            .id();
        app.world_mut()
            .entity_mut(late_target)
            .insert(OperationTarget::new(target_a));
        assert_eq!(
            app.world().get::<OperationState>(replacement),
            Some(&OperationState::Canceled)
        );
        assert_eq!(
            app.world().get::<OperationState>(late_target),
            Some(&OperationState::Pending)
        );
    }

    #[test]
    fn timeout_fails_expired_operations_and_cleans_up_early_completions() {
        let clock = Clock::fake(1_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .add_plugins(OperationPlugin);

        let timed_out = app
            .world_mut()
            .spawn((
                Operation::bundle("slow").unwrap().running(),
                OperationTimeout::from_secs(5),
            ))
            .id();
        let finished_early = app
            .world_mut()
            .spawn((
                Operation::bundle("fast").unwrap().running(),
                OperationTimeout::from_secs(5),
            ))
            .id();
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            2
        );

        clock.advance(Duration::from_secs(2));
        app.world_mut().trigger(OperationCompleted {
            entity: finished_early,
            result: Ok(()),
        });
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(finished_early),
            Some(&OperationState::Succeeded)
        );
        assert_eq!(
            app.world().get::<OperationState>(timed_out),
            Some(&OperationState::Running)
        );
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            1
        );

        clock.advance(Duration::from_secs(3));
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(timed_out),
            Some(&OperationState::Failed)
        );
        assert_eq!(
            app.world()
                .get::<OperationError>(timed_out)
                .unwrap()
                .message(),
            "Operation timed out"
        );
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            0
        );
    }

    #[test]
    fn retry_reschedules_with_exponential_backoff_and_fails_when_exhausted() {
        let clock = Clock::fake(2_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .add_plugins(OperationPlugin);

        let target = app.world_mut().spawn_empty().id();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("upload").unwrap(),
                OperationTarget::new(target),
                OperationPolicy::SerializePerTarget,
                OperationTimeout::from_secs(10),
                OperationRetry::exponential(Duration::from_secs(1), Duration::from_secs(4))
                    .with_max_attempts(2),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Running)
        );
        assert!(app.world().get::<OperationAdmitted>(operation).is_some());

        app.world_mut()
            .trigger(OperationRetryRequested::new(operation, "temporary 503"));
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Pending)
        );
        assert!(app.world().get::<OperationAdmitted>(operation).is_none());
        assert!(
            app.world()
                .get::<OperationRetryWaiting>(operation)
                .is_some()
        );
        assert_eq!(
            app.world()
                .get::<OperationRetry>(operation)
                .unwrap()
                .attempts(),
            1
        );

        clock.advance(Duration::from_millis(999));
        app.update();
        assert!(
            app.world()
                .get::<OperationRetryWaiting>(operation)
                .is_some()
        );
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Pending)
        );

        clock.advance(Duration::from_millis(1));
        app.update();
        assert!(
            app.world()
                .get::<OperationRetryWaiting>(operation)
                .is_none()
        );
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Running)
        );
        assert!(app.world().get::<OperationAdmitted>(operation).is_some());
        assert!(app.world().get::<OperationError>(operation).is_none());

        app.world_mut()
            .trigger(OperationRetryRequested::new(operation, "still 503"));
        app.update();
        assert_eq!(
            app.world()
                .get::<OperationRetry>(operation)
                .unwrap()
                .attempts(),
            2
        );
        assert!(
            app.world()
                .get::<OperationRetryWaiting>(operation)
                .is_some()
        );

        clock.advance(Duration::from_secs(1));
        app.update();
        assert!(
            app.world()
                .get::<OperationRetryWaiting>(operation)
                .is_some()
        );

        clock.advance(Duration::from_secs(1));
        app.update();
        assert!(
            app.world()
                .get::<OperationRetryWaiting>(operation)
                .is_none()
        );
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Running)
        );

        app.world_mut()
            .trigger(OperationRetryRequested::new(operation, "final 503"));
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert_eq!(
            app.world()
                .get::<OperationError>(operation)
                .unwrap()
                .message(),
            "final 503"
        );
        assert!(
            app.world()
                .get::<OperationRetryWaiting>(operation)
                .is_none()
        );
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            0
        );
    }

    #[test]
    fn target_despawn_cancels_policy_operations_and_preserves_unmanaged_operations() {
        let clock = Clock::fake(3_000);
        let mut app = App::new();
        app.insert_resource(clock).add_plugins(OperationPlugin);

        let target = app.world_mut().spawn_empty().id();
        let managed = app
            .world_mut()
            .spawn((
                Operation::bundle("managed").unwrap(),
                OperationTarget::new(target),
                OperationPolicy::SerializePerTarget,
                OperationTimeout::from_secs(10),
            ))
            .id();
        let unmanaged = app
            .world_mut()
            .spawn((
                Operation::bundle("cleanup").unwrap().running(),
                OperationTarget::new(target),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(managed),
            Some(&OperationState::Running)
        );
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            1
        );

        app.world_mut().despawn(target);
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(managed),
            Some(&OperationState::Canceled)
        );
        assert!(app.world().get::<OperationAdmitted>(managed).is_none());
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            0
        );
        assert_eq!(
            app.world().get::<OperationState>(unmanaged),
            Some(&OperationState::Running)
        );
    }

    #[test]
    fn canceled_retry_backoff_cannot_revive_and_older_supersede_does_not_cancel_newer_unmanaged() {
        let clock = Clock::fake(4_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .add_plugins(OperationPlugin);

        let target = app.world_mut().spawn_empty().id();
        let older = app
            .world_mut()
            .spawn((
                Operation::bundle("sync").unwrap().running(),
                OperationTarget::new(target),
                OperationPolicy::SupersedePrevious,
            ))
            .id();
        let newer_unmanaged = app
            .world_mut()
            .spawn((
                Operation::bundle("sync").unwrap().running(),
                OperationTarget::new(target),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(older),
            Some(&OperationState::Running)
        );
        assert_eq!(
            app.world().get::<OperationState>(newer_unmanaged),
            Some(&OperationState::Running)
        );

        let retried = app
            .world_mut()
            .spawn((
                Operation::bundle("push").unwrap().running(),
                OperationRetry::exponential(Duration::from_secs(2), Duration::from_secs(8))
                    .with_max_attempts(3),
            ))
            .id();
        app.world_mut()
            .trigger(OperationRetryRequested::new(retried, "temporary"));
        app.update();
        assert!(app.world().get::<OperationRetryWaiting>(retried).is_some());

        app.world_mut()
            .entity_mut(retried)
            .insert(OperationState::Canceled);
        app.update();
        assert!(app.world().get::<OperationRetryWaiting>(retried).is_none());
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            0
        );

        clock.advance(Duration::from_secs(5));
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(retried),
            Some(&OperationState::Canceled)
        );
    }
}
