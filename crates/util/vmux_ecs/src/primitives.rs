use bevy::prelude::*;
use moonshine_save::prelude::Save;

use super::store::StoreAppExt;
use crate::PageMetadata;
use crate::WorkspaceKind;
use crate::component::{
    ActivateRequest, Active, ArchivedAt, CollectionView, CreatedAt, Cwd, Description,
    LastActivatedAt, Order,
};
use vmux_api::{BuiltinIcon, PageIcon};

use bevy::ecs::relationship::Relationship;

pub struct PrimitivesPlugin;

impl Plugin for PrimitivesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActivationClock>()
            .register_stored::<PageMetadata>()
            .register_stored::<WorkspaceKind>()
            .register_type::<PageIcon>()
            .register_type::<BuiltinIcon>()
            .register_stored::<Cwd>()
            .register_stored::<Description>()
            .register_stored::<CollectionView>()
            .register_stored::<CreatedAt>()
            .register_stored::<LastActivatedAt>()
            .register_stored::<ArchivedAt>()
            .register_stored::<Order>()
            .register_type::<Active>()
            .register_type::<Children>()
            .register_type::<ChildOf>()
            .track_store::<Save>()
            .add_observer(activate);
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
    fn registers_shared_components() {
        let mut app = App::new();
        app.add_plugins(PrimitivesPlugin);

        let registry = app.world().resource::<AppTypeRegistry>().read();
        let filter = super::super::store::WorkspaceStored::filter(&registry);
        assert!(
            registry
                .get(std::any::TypeId::of::<PageMetadata>())
                .is_some()
        );
        assert!(registry.get(std::any::TypeId::of::<CreatedAt>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<ArchivedAt>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Order>()).is_some());
        assert!(filter.is_allowed::<Cwd>());
        assert!(filter.is_allowed::<Description>());
        assert!(filter.is_allowed::<CollectionView>());
        assert!(filter.is_allowed::<WorkspaceKind>());
    }

    #[test]
    fn active_marker_is_registered_and_reflectable() {
        let mut app = App::new();
        app.add_plugins(PrimitivesPlugin);

        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(registry.get(std::any::TypeId::of::<Active>()).is_some());
    }

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
