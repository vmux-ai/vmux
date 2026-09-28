use bevy::prelude::*;
use bevy::reflect::{FromType, GetTypeRegistration, TypePath, TypeRegistry};
use bevy_world_serialization::WorldFilter;

#[derive(Clone)]
pub struct WorkspacePersisted;

impl<T> FromType<T> for WorkspacePersisted {
    fn from_type() -> Self {
        Self
    }
}

#[derive(Event, Clone, Copy, Debug, Default)]
pub struct PersistenceDirty;

pub trait PersistenceAppExt {
    fn register_persisted<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration;

    fn track_persistence<T>(&mut self) -> &mut Self
    where
        T: Component;
}

impl PersistenceAppExt for App {
    fn register_persisted<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration,
    {
        self.register_type::<T>()
            .register_type_data::<T, WorkspacePersisted>()
            .track_persistence::<T>()
    }

    fn track_persistence<T>(&mut self) -> &mut Self
    where
        T: Component,
    {
        self.add_systems(Update, detect_persistence_change::<T>)
    }
}

fn detect_persistence_change<T: Component>(
    changed: Query<(), Or<(Added<T>, Changed<T>)>>,
    mut removed: RemovedComponents<T>,
    mut commands: Commands,
) {
    if !changed.is_empty() || removed.read().next().is_some() {
        commands.trigger(PersistenceDirty);
    }
}

pub fn persisted_components(registry: &TypeRegistry) -> WorldFilter {
    let mut filter = WorldFilter::deny_all();
    for registration in registry.iter() {
        if registration.data::<WorkspacePersisted>().is_some() {
            filter = filter.allow_by_id(registration.type_id());
        }
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Saved;

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Transient;

    #[test]
    fn feature_registration_builds_the_component_allowlist() {
        let mut app = App::new();
        app.register_persisted::<Saved>()
            .register_type::<Transient>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let filter = persisted_components(&registry);

        assert!(filter.is_allowed::<Saved>());
        assert!(!filter.is_allowed::<Transient>());
    }
}
