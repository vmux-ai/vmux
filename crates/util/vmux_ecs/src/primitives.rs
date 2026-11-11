use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::relationship::Relationship;

use crate::{
    ActivateRequest, Active, ArchivedAt, CollectionView, CreatedAt, Cwd, Description,
    LastActivatedAt, LastVisitedAt, Order, Uuid, WorkspaceKind,
};

pub struct PrimitivesPlugin;

impl Plugin for PrimitivesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActivationClock>()
            .register_type::<Cwd>()
            .register_type::<Description>()
            .register_type::<CollectionView>()
            .register_type::<CreatedAt>()
            .register_type::<LastActivatedAt>()
            .register_type::<LastVisitedAt>()
            .register_type::<ArchivedAt>()
            .register_type::<Order>()
            .register_type::<Active>()
            .register_type::<Uuid>()
            .register_type::<WorkspaceKind>()
            .register_type::<Children>()
            .register_type::<ChildOf>()
            .add_observer(activate);
        #[cfg(feature = "contracts")]
        app.register_type::<crate::PageMetadata>()
            .register_type::<vmux_api::PageIcon>()
            .register_type::<vmux_api::BuiltinIcon>();
    }
}

#[derive(Resource, Default)]
struct ActivationClock(i64);

impl ActivationClock {
    fn next(&mut self, after: i64) -> LastActivatedAt {
        self.0 = LastActivatedAt::now()
            .0
            .max(self.0.saturating_add(1))
            .max(after.saturating_add(1));
        LastActivatedAt(self.0)
    }
}

fn activate(
    trigger: On<ActivateRequest>,
    child_of: Query<&ChildOf>,
    timestamps: Query<&LastActivatedAt>,
    mut clock: ResMut<ActivationClock>,
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
    let activated_at = clock.next(after);
    for entity in path {
        commands.entity(entity).insert(activated_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_propagates_through_ancestors() {
        let mut app = App::new();
        app.add_plugins(PrimitivesPlugin);

        let root = app.world_mut().spawn(LastActivatedAt(1)).id();
        let child = app
            .world_mut()
            .spawn((LastActivatedAt(1), ChildOf(root)))
            .id();
        let leaf = app
            .world_mut()
            .spawn((LastActivatedAt(1), ChildOf(child)))
            .id();

        app.world_mut().trigger(ActivateRequest { entity: leaf });
        app.update();

        assert!(app.world().get::<LastActivatedAt>(root).unwrap().0 > 1);
        assert!(app.world().get::<LastActivatedAt>(child).unwrap().0 > 1);
        assert!(app.world().get::<LastActivatedAt>(leaf).unwrap().0 > 1);
    }

    #[test]
    fn repeated_activation_orders_siblings_even_when_the_clock_has_not_advanced() {
        let mut app = App::new();
        app.add_plugins(PrimitivesPlugin);
        let restored = LastActivatedAt(LastActivatedAt::now().0 + 5_000);
        let root = app.world_mut().spawn(restored).id();
        let first = app.world_mut().spawn((restored, ChildOf(root))).id();
        let second = app.world_mut().spawn((restored, ChildOf(root))).id();
        app.world_mut().trigger(ActivateRequest { entity: first });
        app.world_mut().trigger(ActivateRequest { entity: second });
        app.update();
        let first_at = app.world().get::<LastActivatedAt>(first).unwrap().0;
        let second_at = app.world().get::<LastActivatedAt>(second).unwrap().0;
        assert!(first_at > restored.0);
        assert!(second_at > first_at);
        assert_eq!(
            app.world().get::<LastActivatedAt>(root).unwrap().0,
            second_at
        );
    }
}
