use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};
use serde::{Deserialize, Serialize};

const RETENTION: Duration = Duration::from_secs(300);

pub struct OperationPlugin;

impl Plugin for OperationPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Operation>()
            .register_type::<OperationKind>()
            .register_type::<OperationState>()
            .register_type::<OperationTargetRevision>()
            .register_type::<OperationError>()
            .init_resource::<OperationExpiry>()
            .add_systems(PostUpdate, (finish, cleanup).chain());
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Operation;

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
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
}

#[derive(Component)]
struct OperationExpiresAt(Instant);

#[derive(Resource, Default)]
struct OperationExpiry(Option<Instant>);

type ChangedOperations<'w, 's> =
    Query<'w, 's, (Entity, &'static OperationState), (With<Operation>, Changed<OperationState>)>;

fn finish(
    operations: ChangedOperations,
    mut expiry: ResMut<OperationExpiry>,
    mut commands: Commands,
) {
    for (entity, state) in &operations {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            commands.entity(entity).remove::<OperationExpiresAt>();
        } else {
            let deadline = Instant::now() + RETENTION;
            if expiry.0.is_none_or(|current| deadline < current) {
                expiry.0 = Some(deadline);
            }
            commands.entity(entity).insert(OperationExpiresAt(deadline));
        }
    }
}

fn cleanup(
    operations: Query<(Entity, &OperationExpiresAt), With<Operation>>,
    mut expiry: ResMut<OperationExpiry>,
    mut commands: Commands,
) {
    let now = Instant::now();
    if expiry.0.is_none_or(|deadline| now < deadline) {
        return;
    }
    let mut next = None::<Instant>;
    for (entity, finished) in &operations {
        if finished.0 <= now {
            commands.entity(entity).despawn();
        } else if next.is_none_or(|deadline| finished.0 < deadline) {
            next = Some(finished.0);
        }
    }
    expiry.0 = next;
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
