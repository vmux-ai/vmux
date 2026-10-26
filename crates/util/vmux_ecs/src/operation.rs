use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};
use serde::{Deserialize, Serialize};

use crate::Revision;

const RETENTION: Duration = Duration::from_secs(300);

pub struct OperationPlugin;

impl Plugin for OperationPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Operation>()
            .register_type::<OperationKind>()
            .register_type::<OperationState>()
            .register_type::<OperationTarget>()
            .register_type::<OperationTargetRevision>()
            .register_type::<OperationError>()
            .init_resource::<OperationExpiry>()
            .add_systems(Update, (finish, cleanup).chain());
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

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct OperationTarget(pub Entity);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct OperationTargetRevision(pub Revision);

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
    fn terminal_operations_receive_a_retention_deadline() {
        let mut app = App::new();
        app.add_plugins(OperationPlugin);
        let operation = app
            .world_mut()
            .spawn((Operation, OperationState::Succeeded))
            .id();

        app.update();

        assert!(app.world().get::<OperationExpiresAt>(operation).is_some());
    }
}
