use bevy::prelude::*;
use moonshine_save::prelude::Save;

use super::persistence::PersistenceAppExt;
use crate::PageMetadata;
use crate::component::{ActivateRequest, Active, CreatedAt, LastActivatedAt, Order};
use vmux_api::{BuiltinIcon, PageIcon};

use bevy::ecs::relationship::Relationship;

pub struct PrimitivesPlugin;

impl Plugin for PrimitivesPlugin {
    fn build(&self, app: &mut App) {
        app.register_persisted::<PageMetadata>()
            .register_type::<PageIcon>()
            .register_type::<BuiltinIcon>()
            .register_persisted::<CreatedAt>()
            .register_persisted::<LastActivatedAt>()
            .register_persisted::<Order>()
            .register_type::<Active>()
            .register_type::<Children>()
            .register_type::<ChildOf>()
            .track_persistence::<Save>()
            .track_persistence::<Name>()
            .track_persistence::<Children>()
            .track_persistence::<ChildOf>()
            .add_observer(activate);
    }
}

fn activate(trigger: On<ActivateRequest>, child_of: Query<&ChildOf>, mut commands: Commands) {
    let activated_at = LastActivatedAt::now();
    let mut current = trigger.event_target();
    commands.entity(current).insert(activated_at);
    while let Ok(parent_rel) = child_of.get(current) {
        current = parent_rel.get();
        commands.entity(current).insert(activated_at);
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
        assert!(
            registry
                .get(std::any::TypeId::of::<PageMetadata>())
                .is_some()
        );
        assert!(registry.get(std::any::TypeId::of::<CreatedAt>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Order>()).is_some());
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
}
