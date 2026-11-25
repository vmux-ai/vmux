use std::collections::HashSet;
use std::fmt;

use bevy_app::{App, Plugin};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::relationship::RelationshipTarget;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};

use crate::{Operation, OperationState, TargetedOperations};

pub struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Loading>()
            .register_type::<Ready>()
            .register_type::<Busy>()
            .register_type::<Failure>()
            .register_type::<Progress>()
            .register_type::<Retired>();
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
#[component(on_insert = on_loading)]
pub struct Loading;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
#[component(on_insert = on_ready)]
pub struct Ready;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Busy;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
#[component(on_insert = on_failure)]
pub struct Failure(pub String);

impl Failure {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Progress {
    pub completed: u64,
    pub total: Option<u64>,
    pub label: Option<String>,
}

impl Progress {
    pub const fn new(completed: u64, total: Option<u64>) -> Self {
        Self {
            completed,
            total,
            label: None,
        }
    }

    pub const fn indeterminate() -> Self {
        Self {
            completed: 0,
            total: None,
            label: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn fraction(&self) -> Option<f32> {
        let total = self.total?;
        if total == 0 {
            return None;
        }
        Some((self.completed.min(total) as f64 / total as f64) as f32)
    }

    pub const fn is_complete(&self) -> bool {
        match self.total {
            Some(total) => self.completed >= total,
            None => false,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
#[component(on_insert = on_retired)]
pub struct Retired;

fn on_loading(mut world: DeferredWorld, context: HookContext) {
    let entity = context.entity;
    if world.get::<Ready>(entity).is_some() || world.get::<Failure>(entity).is_some() {
        world.commands().entity(entity).remove::<(Ready, Failure)>();
    }
}

fn on_ready(mut world: DeferredWorld, context: HookContext) {
    let entity = context.entity;
    if world.get::<Loading>(entity).is_some() || world.get::<Failure>(entity).is_some() {
        world
            .commands()
            .entity(entity)
            .remove::<(Loading, Failure)>();
    }
}

fn on_failure(mut world: DeferredWorld, context: HookContext) {
    let entity = context.entity;
    if world.get::<Loading>(entity).is_some() || world.get::<Ready>(entity).is_some() {
        world.commands().entity(entity).remove::<(Loading, Ready)>();
    }
}

fn on_retired(mut world: DeferredWorld, context: HookContext) {
    let root = context.entity;
    let clear_active = world.get::<Loading>(root).is_some() || world.get::<Busy>(root).is_some();
    let mut visited = HashSet::new();
    let mut stack = vec![root];
    let mut cancel = Vec::new();
    while let Some(current) = stack.pop() {
        if !visited.insert(current) {
            continue;
        }
        if let Some(children) = world.get::<Children>(current) {
            for child in children.iter() {
                if world.get::<Operation>(child).is_some()
                    && matches!(
                        world.get::<OperationState>(child),
                        Some(OperationState::Pending | OperationState::Running)
                    )
                {
                    cancel.push(child);
                }
                stack.push(child);
            }
        }
        if let Some(targeted) = world.get::<TargetedOperations>(current) {
            for operation in targeted.iter() {
                if world.get::<Operation>(operation).is_some()
                    && matches!(
                        world.get::<OperationState>(operation),
                        Some(OperationState::Pending | OperationState::Running)
                    )
                {
                    cancel.push(operation);
                }
            }
        }
    }
    if clear_active {
        world.commands().entity(root).remove::<(Loading, Busy)>();
    }
    let mut canceled = HashSet::new();
    for operation in cancel {
        if canceled.insert(operation) {
            world
                .commands()
                .entity(operation)
                .insert(OperationState::Canceled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OperationPlugin, OperationTarget};

    #[test]
    fn ready_failure_and_loading_enforce_mutual_exclusion() {
        let mut app = App::new();
        app.add_plugins(StatusPlugin);
        let entity = app.world_mut().spawn((Loading, Busy)).id();
        app.world_mut().flush();
        assert!(app.world().get::<Loading>(entity).is_some());
        assert!(app.world().get::<Busy>(entity).is_some());

        app.world_mut().entity_mut(entity).insert(Ready);
        app.world_mut().flush();
        assert!(app.world().get::<Ready>(entity).is_some());
        assert!(app.world().get::<Loading>(entity).is_none());
        assert!(app.world().get::<Failure>(entity).is_none());
        assert!(app.world().get::<Busy>(entity).is_some());

        app.world_mut()
            .entity_mut(entity)
            .insert(Failure::new("disk unavailable"));
        app.world_mut().flush();
        assert_eq!(
            app.world().get::<Failure>(entity).map(Failure::message),
            Some("disk unavailable")
        );
        assert!(app.world().get::<Ready>(entity).is_none());
        assert!(app.world().get::<Loading>(entity).is_none());

        app.world_mut().entity_mut(entity).insert(Loading);
        app.world_mut().flush();
        assert!(app.world().get::<Loading>(entity).is_some());
        assert!(app.world().get::<Failure>(entity).is_none());
        assert!(app.world().get::<Ready>(entity).is_none());
    }

    #[test]
    fn retired_cancels_active_child_and_targeted_operations_without_altering_terminal_work() {
        let mut app = App::new();
        app.add_plugins((StatusPlugin, OperationPlugin));
        let owner = app.world_mut().spawn((Loading, Busy)).id();
        let pending = app
            .world_mut()
            .spawn((Operation::bundle("op.pending").unwrap(), ChildOf(owner)))
            .id();
        let running = app
            .world_mut()
            .spawn((
                Operation::bundle("op.running").unwrap().running(),
                ChildOf(owner),
            ))
            .id();
        let targeted = app
            .world_mut()
            .spawn((
                Operation::bundle("op.targeted").unwrap().running(),
                OperationTarget::new(owner),
            ))
            .id();
        let succeeded = app
            .world_mut()
            .spawn((Operation, OperationState::Succeeded, ChildOf(owner)))
            .id();

        app.world_mut().entity_mut(owner).insert(Retired);
        app.world_mut().flush();

        assert!(app.world().get::<Retired>(owner).is_some());
        assert!(app.world().get::<Loading>(owner).is_none());
        assert!(app.world().get::<Busy>(owner).is_none());
        assert_eq!(
            app.world().get::<OperationState>(pending),
            Some(&OperationState::Canceled)
        );
        assert_eq!(
            app.world().get::<OperationState>(running),
            Some(&OperationState::Canceled)
        );
        assert_eq!(
            app.world().get::<OperationState>(targeted),
            Some(&OperationState::Canceled)
        );
        assert_eq!(
            app.world().get::<OperationState>(succeeded),
            Some(&OperationState::Succeeded)
        );
    }

    #[test]
    fn progress_computes_clamped_fraction_and_completion() {
        let half = Progress::new(5, Some(10)).with_label("Indexing");
        assert_eq!(half.fraction(), Some(0.5));
        assert!(!half.is_complete());
        assert_eq!(half.label.as_deref(), Some("Indexing"));

        let over = Progress::new(15, Some(10));
        assert_eq!(over.fraction(), Some(1.0));
        assert!(over.is_complete());

        let unknown = Progress::indeterminate();
        assert_eq!(unknown.fraction(), None);
        assert!(!unknown.is_complete());
    }
}
