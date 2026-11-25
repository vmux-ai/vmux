use bevy_app::{App, First, Last, Plugin};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::relationship::Relationship;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};
use vmux_time::{Clock, Deadline, DeadlinePlugin, DeadlineReached, DeadlineSet, TimePlugin};
use web_time::{Duration, SystemTime, UNIX_EPOCH};

pub struct TimestampPlugin;

impl Plugin for TimestampPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimePlugin>() {
            app.add_plugins(TimePlugin);
        }
        if !app.is_plugin_added::<DeadlinePlugin<Expiry>>() {
            app.add_plugins(DeadlinePlugin::<Expiry>::default());
        }
        app.init_resource::<ActivationClock>()
            .register_type::<CreatedAt>()
            .register_type::<UpdatedAt>()
            .register_type::<LastActivatedAt>()
            .register_type::<LastVisitedAt>()
            .register_type::<ArchivedAt>()
            .register_type::<ExpiresAt>()
            .add_observer(activate)
            .add_observer(touch)
            .add_observer(archive)
            .add_observer(unarchive)
            .add_observer(on_expiry)
            .add_observer(cancel_expiry)
            .add_systems(First, sweep_expired.after(DeadlineSet::Fire))
            .add_systems(Last, arm_expiry.before(DeadlineSet::Sync));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnixMillis(pub i64);

impl UnixMillis {
    pub fn now() -> Self {
        Self(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        )
    }

    pub fn from_clock(clock: &Clock) -> Self {
        Self(clock.unix_millis())
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_history"]
#[component(on_add = stamp_created)]
pub struct CreatedAt(pub i64);

impl CreatedAt {
    pub fn now() -> Self {
        Self(UnixMillis::now().0)
    }

    pub fn from_clock(clock: &Clock) -> Self {
        Self(clock.unix_millis())
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct UpdatedAt(pub i64);

impl UpdatedAt {
    pub fn now() -> Self {
        Self(UnixMillis::now().0)
    }

    pub fn from_clock(clock: &Clock) -> Self {
        Self(clock.unix_millis())
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_history"]
pub struct LastActivatedAt(pub i64);

impl LastActivatedAt {
    pub fn now() -> Self {
        Self(UnixMillis::now().0)
    }

    pub fn from_clock(clock: &Clock) -> Self {
        Self(clock.unix_millis())
    }

    pub fn latest(entities: impl Iterator<Item = (Entity, Self)>) -> Option<Entity> {
        entities
            .max_by_key(|(_, activated_at)| activated_at.0)
            .map(|(entity, _)| entity)
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_history"]
pub struct LastVisitedAt(pub i64);

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct ArchivedAt(pub i64);

impl ArchivedAt {
    pub fn now() -> Self {
        Self(UnixMillis::now().0)
    }

    pub fn from_clock(clock: &Clock) -> Self {
        Self(clock.unix_millis())
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct ExpiresAt(pub i64);

impl ExpiresAt {
    pub fn after(clock: &Clock, duration: Duration) -> Self {
        let delta = duration.as_millis().min(i64::MAX as u128) as i64;
        Self(clock.unix_millis().saturating_add(delta))
    }

    pub fn is_expired(&self, clock: &Clock) -> bool {
        clock.unix_millis() >= self.0
    }

    fn remaining(&self, clock: &Clock) -> Option<Duration> {
        let diff = self.0.saturating_sub(clock.unix_millis());
        if diff <= 0 {
            return None;
        }
        Some(Duration::from_millis(diff as u64))
    }
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ActivateRequest {
    #[event_target]
    pub entity: Entity,
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct Touch {
    #[event_target]
    pub entity: Entity,
    pub propagate: bool,
}

impl Touch {
    pub fn entity(entity: Entity) -> Self {
        Self {
            entity,
            propagate: false,
        }
    }

    pub fn with_ancestors(entity: Entity) -> Self {
        Self {
            entity,
            propagate: true,
        }
    }
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ArchiveRequest {
    #[event_target]
    pub entity: Entity,
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct UnarchiveRequest {
    #[event_target]
    pub entity: Entity,
}

struct Expiry;

#[derive(Resource, Default)]
struct ActivationClock(i64);

impl ActivationClock {
    fn next(&mut self, now: i64, after: i64) -> LastActivatedAt {
        self.0 = now
            .max(self.0.saturating_add(1))
            .max(after.saturating_add(1));
        LastActivatedAt(self.0)
    }
}

fn stamp_created(mut world: DeferredWorld, context: HookContext) {
    let current = world.get::<CreatedAt>(context.entity).map(|at| at.0);
    if current != Some(0) {
        return;
    }
    let now = world
        .get_resource::<Clock>()
        .map_or_else(|| UnixMillis::now().0, Clock::unix_millis);
    if let Some(mut created) = world.get_mut::<CreatedAt>(context.entity) {
        created.0 = now;
    }
}

fn activate(
    trigger: On<ActivateRequest>,
    child_of: Query<&ChildOf>,
    timestamps: Query<&LastActivatedAt>,
    clock: Option<Res<Clock>>,
    mut sequence: ResMut<ActivationClock>,
    mut commands: Commands,
) {
    let mut current = trigger.event_target();
    let mut path = vec![current];
    while let Ok(parent_rel) = child_of.get(current) {
        current = parent_rel.get();
        path.push(current);
    }
    let mut after = 0;
    for entity in &path {
        if let Ok(timestamp) = timestamps.get(*entity) {
            after = after.max(timestamp.0);
        }
    }
    let now = clock.map_or_else(|| UnixMillis::now().0, |clock| clock.unix_millis());
    let activated_at = sequence.next(now, after);
    for entity in path {
        commands.entity(entity).insert(activated_at);
    }
}

fn touch(
    trigger: On<Touch>,
    child_of: Query<&ChildOf>,
    clock: Option<Res<Clock>>,
    mut commands: Commands,
) {
    let now = UpdatedAt(clock.map_or_else(|| UnixMillis::now().0, |clock| clock.unix_millis()));
    let mut current = trigger.event_target();
    commands.entity(current).try_insert(now);
    if !trigger.propagate {
        return;
    }
    while let Ok(parent) = child_of.get(current) {
        current = parent.get();
        commands.entity(current).try_insert(now);
    }
}

fn archive(trigger: On<ArchiveRequest>, clock: Option<Res<Clock>>, mut commands: Commands) {
    let now = ArchivedAt(clock.map_or_else(|| UnixMillis::now().0, |clock| clock.unix_millis()));
    commands.entity(trigger.event_target()).try_insert(now);
}

fn unarchive(trigger: On<UnarchiveRequest>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .try_remove::<ArchivedAt>();
}

fn arm_expiry(
    clock: Res<Clock>,
    changed: Query<(Entity, &ExpiresAt), Changed<ExpiresAt>>,
    mut commands: Commands,
) {
    for (entity, expires) in &changed {
        match expires.remaining(&clock) {
            Some(remaining) => {
                commands
                    .entity(entity)
                    .try_insert(Deadline::<Expiry>::after(&clock, remaining));
            }
            None => {
                commands.entity(entity).try_despawn();
            }
        }
    }
}

fn on_expiry(
    reached: On<DeadlineReached<Expiry>>,
    clock: Res<Clock>,
    expires: Query<&ExpiresAt>,
    mut commands: Commands,
) {
    let entity = reached.entity;
    let Ok(expires) = expires.get(entity) else {
        return;
    };
    match expires.remaining(&clock) {
        Some(remaining) => {
            commands
                .entity(entity)
                .try_insert(Deadline::<Expiry>::after(&clock, remaining));
        }
        None => {
            commands.entity(entity).try_despawn();
        }
    }
}

fn cancel_expiry(removed: On<Remove, ExpiresAt>, mut commands: Commands) {
    commands
        .entity(removed.entity)
        .try_remove::<Deadline<Expiry>>();
}

fn sweep_expired(clock: Res<Clock>, entities: Query<(Entity, &ExpiresAt)>, mut commands: Commands) {
    for (entity, expires) in &entities {
        if expires.is_expired(&clock) {
            commands.entity(entity).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_at_auto_stamps_unset_values_and_preserves_explicit_timestamps() {
        let clock = Clock::fake(1_700_000_000_000);
        let mut app = App::new();
        app.insert_resource(clock).add_plugins(TimestampPlugin);

        let auto = app.world_mut().spawn(CreatedAt::default()).id();
        let restored = app.world_mut().spawn(CreatedAt(42_000)).id();

        assert_eq!(
            app.world().get::<CreatedAt>(auto),
            Some(&CreatedAt(1_700_000_000_000))
        );
        assert_eq!(
            app.world().get::<CreatedAt>(restored),
            Some(&CreatedAt(42_000))
        );
    }

    #[test]
    fn touch_updates_updated_at_with_optional_ancestor_propagation() {
        let clock = Clock::fake(10_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .add_plugins(TimestampPlugin);

        let root = app.world_mut().spawn_empty().id();
        let child = app.world_mut().spawn(ChildOf(root)).id();

        app.world_mut().trigger(Touch::entity(child));
        app.update();
        assert_eq!(
            app.world().get::<UpdatedAt>(child),
            Some(&UpdatedAt(10_000))
        );
        assert_eq!(app.world().get::<UpdatedAt>(root), None);

        clock.advance(Duration::from_secs(5));
        app.world_mut().trigger(Touch::with_ancestors(child));
        app.update();
        assert_eq!(
            app.world().get::<UpdatedAt>(child),
            Some(&UpdatedAt(15_000))
        );
        assert_eq!(app.world().get::<UpdatedAt>(root), Some(&UpdatedAt(15_000)));
    }

    #[test]
    fn archive_and_unarchive_toggle_archived_at() {
        let clock = Clock::fake(50_000);
        let mut app = App::new();
        app.insert_resource(clock).add_plugins(TimestampPlugin);

        let entity = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(ArchiveRequest { entity });
        app.update();
        assert_eq!(
            app.world().get::<ArchivedAt>(entity),
            Some(&ArchivedAt(50_000))
        );

        app.world_mut().trigger(UnarchiveRequest { entity });
        app.update();
        assert_eq!(app.world().get::<ArchivedAt>(entity), None);
    }

    #[test]
    fn expires_at_despawns_when_clock_reaches_expiry_and_respects_extension_and_removal() {
        let clock = Clock::fake(100_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .add_plugins(TimestampPlugin);

        let short = app
            .world_mut()
            .spawn(ExpiresAt::after(&clock, Duration::from_secs(5)))
            .id();
        let extended = app
            .world_mut()
            .spawn(ExpiresAt::after(&clock, Duration::from_secs(5)))
            .id();
        let cleared = app
            .world_mut()
            .spawn(ExpiresAt::after(&clock, Duration::from_secs(5)))
            .id();
        app.update();

        app.world_mut()
            .entity_mut(extended)
            .insert(ExpiresAt::after(&clock, Duration::from_secs(12)));
        app.world_mut().entity_mut(cleared).remove::<ExpiresAt>();
        app.update();

        clock.advance(Duration::from_secs(5));
        app.update();

        assert!(app.world().get_entity(short).is_err());
        assert!(app.world().get_entity(extended).is_ok());
        assert!(app.world().get_entity(cleared).is_ok());

        clock.advance(Duration::from_secs(7));
        app.update();

        assert!(app.world().get_entity(extended).is_err());
        assert!(app.world().get_entity(cleared).is_ok());
    }

    #[test]
    fn legacy_history_type_paths_remain_preserved_for_store_compatibility() {
        assert_eq!(
            <CreatedAt as bevy_reflect::TypePath>::type_path(),
            "vmux_history::CreatedAt"
        );
        assert_eq!(
            <LastActivatedAt as bevy_reflect::TypePath>::type_path(),
            "vmux_history::LastActivatedAt"
        );
        assert_eq!(
            <LastVisitedAt as bevy_reflect::TypePath>::type_path(),
            "vmux_history::LastVisitedAt"
        );
        assert_eq!(
            <ArchivedAt as bevy_reflect::TypePath>::type_path(),
            "vmux_ecs::ArchivedAt"
        );
        assert_eq!(
            <UpdatedAt as bevy_reflect::TypePath>::type_path(),
            "vmux_ecs::UpdatedAt"
        );
        assert_eq!(
            <ExpiresAt as bevy_reflect::TypePath>::type_path(),
            "vmux_ecs::ExpiresAt"
        );
    }
}
