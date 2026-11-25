use std::marker::PhantomData;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};

use crate::{Collapsed, Pin};

pub struct TogglePlugin<T: Component + Default>(PhantomData<fn() -> T>);

impl<T: Component + Default> Default for TogglePlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Component + Default> Plugin for TogglePlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_observer(flip::<T>);
    }
}

pub struct TogglesPlugin;

impl Plugin for TogglesPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Pin>()
            .register_type::<Collapsed>()
            .register_type::<Disabled>();
        if !app.is_plugin_added::<TogglePlugin<Pin>>() {
            app.add_plugins(TogglePlugin::<Pin>::default());
        }
        if !app.is_plugin_added::<TogglePlugin<Collapsed>>() {
            app.add_plugins(TogglePlugin::<Collapsed>::default());
        }
        if !app.is_plugin_added::<TogglePlugin<Disabled>>() {
            app.add_plugins(TogglePlugin::<Disabled>::default());
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Disabled;

#[derive(EntityEvent, Debug)]
pub struct Toggle<T: Component + Default> {
    #[event_target]
    pub entity: Entity,
    marker: PhantomData<fn() -> T>,
}

impl<T: Component + Default> Toggle<T> {
    pub const fn new(entity: Entity) -> Self {
        Self {
            entity,
            marker: PhantomData,
        }
    }
}

impl<T: Component + Default> Clone for Toggle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Component + Default> Copy for Toggle<T> {}

impl<T: Component + Default> PartialEq for Toggle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.entity == other.entity
    }
}

impl<T: Component + Default> Eq for Toggle<T> {}

#[derive(EntityEvent, Debug)]
pub struct Toggled<T: Component> {
    #[event_target]
    pub entity: Entity,
    pub enabled: bool,
    marker: PhantomData<fn() -> T>,
}

impl<T: Component> Clone for Toggled<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Component> Copy for Toggled<T> {}

impl<T: Component> PartialEq for Toggled<T> {
    fn eq(&self, other: &Self) -> bool {
        self.entity == other.entity && self.enabled == other.enabled
    }
}

impl<T: Component> Eq for Toggled<T> {}

fn flip<T: Component + Default>(
    trigger: On<Toggle<T>>,
    entities: Query<Has<T>>,
    mut commands: Commands,
) {
    let entity = trigger.event().entity;
    let Ok(present) = entities.get(entity) else {
        return;
    };
    if present {
        commands.entity(entity).remove::<T>();
    } else {
        commands.entity(entity).insert(T::default());
    }
    commands.trigger(Toggled::<T> {
        entity,
        enabled: !present,
        marker: PhantomData,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Recorded<T: Component> {
        events: Vec<(Entity, bool)>,
        marker: PhantomData<fn() -> T>,
    }

    fn record<T: Component>(trigger: On<Toggled<T>>, mut recorded: ResMut<Recorded<T>>) {
        let event = trigger.event();
        recorded.events.push((event.entity, event.enabled));
    }

    #[test]
    fn toggles_plugin_flips_pin_collapsed_and_disabled_markers() {
        let mut app = App::new();
        app.add_plugins(TogglesPlugin)
            .init_resource::<Recorded<Pin>>()
            .init_resource::<Recorded<Collapsed>>()
            .init_resource::<Recorded<Disabled>>()
            .add_observer(record::<Pin>)
            .add_observer(record::<Collapsed>)
            .add_observer(record::<Disabled>);
        let entity = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(Toggle::<Pin>::new(entity));
        app.world_mut().trigger(Toggle::<Collapsed>::new(entity));
        app.world_mut().trigger(Toggle::<Disabled>::new(entity));
        app.world_mut().flush();

        assert!(app.world().get::<Pin>(entity).is_some());
        assert!(app.world().get::<Collapsed>(entity).is_some());
        assert!(app.world().get::<Disabled>(entity).is_some());

        app.world_mut().trigger(Toggle::<Pin>::new(entity));
        app.world_mut().trigger(Toggle::<Collapsed>::new(entity));
        app.world_mut().trigger(Toggle::<Disabled>::new(entity));
        app.world_mut().flush();

        assert!(app.world().get::<Pin>(entity).is_none());
        assert!(app.world().get::<Collapsed>(entity).is_none());
        assert!(app.world().get::<Disabled>(entity).is_none());
        assert_eq!(
            app.world().resource::<Recorded<Pin>>().events,
            vec![(entity, true), (entity, false)]
        );
        assert_eq!(
            app.world().resource::<Recorded<Collapsed>>().events,
            vec![(entity, true), (entity, false)]
        );
        assert_eq!(
            app.world().resource::<Recorded<Disabled>>().events,
            vec![(entity, true), (entity, false)]
        );
    }

    #[test]
    fn toggle_on_missing_entity_is_ignored() {
        let mut app = App::new();
        app.add_plugins(TogglePlugin::<Disabled>::default())
            .init_resource::<Recorded<Disabled>>()
            .add_observer(record::<Disabled>);
        let entity = app.world_mut().spawn_empty().id();
        app.world_mut().despawn(entity);

        app.world_mut().trigger(Toggle::<Disabled>::new(entity));
        app.world_mut().flush();

        assert!(
            app.world()
                .resource::<Recorded<Disabled>>()
                .events
                .is_empty()
        );
    }
}
