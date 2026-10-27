use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
#[cfg(test)]
use bevy_ecs::reflect::AppTypeRegistry;
use bevy_ecs::system::SystemParam;
use bevy_reflect::{FromType, GetTypeRegistration, Reflect, TypePath, TypeRegistry};
use bevy_world_serialization::WorldFilter;
use std::marker::PhantomData;
use std::path::PathBuf;

pub struct PersistenceDomain<D>(PhantomData<fn() -> D>);

impl<D> Clone for PersistenceDomain<D> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<T, D: 'static> FromType<T> for PersistenceDomain<D> {
    fn from_type() -> Self {
        Self(PhantomData)
    }
}

impl<D: 'static> PersistenceDomain<D> {
    pub fn filter(registry: &TypeRegistry) -> WorldFilter {
        let mut filter = WorldFilter::deny_all();
        for registration in registry.iter() {
            if registration.data::<Self>().is_some() {
                filter = filter.allow_by_id(registration.type_id());
            }
        }
        filter
    }
}

pub enum WorkspacePersistence {}

pub type WorkspacePersisted = PersistenceDomain<WorkspacePersistence>;

#[derive(Event, Clone, Copy, Debug, Default)]
pub struct PersistenceDirty;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PageRestore;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WorkspaceRestore {
    pub store_present: bool,
    pub complete: bool,
}

#[derive(Message, Clone)]
pub struct WorkspaceSaveRequest {
    pub path: PathBuf,
}

#[derive(Component)]
pub struct WorkspaceStoreValidator {
    pub name: &'static str,
    pub rejects: fn(&str) -> bool,
}

#[derive(SystemParam)]
pub struct WorkspaceStoreValidators<'w, 's> {
    validators: Query<'w, 's, &'static WorkspaceStoreValidator>,
}

impl WorkspaceStoreValidators<'_, '_> {
    pub fn rejected_by(&self, body: &str) -> Option<&'static str> {
        self.validators
            .iter()
            .find(|validator| (validator.rejects)(body))
            .map(|validator| validator.name)
    }
}

pub trait PersistenceAppExt {
    fn register_persistence_type<D, T>(&mut self) -> &mut Self
    where
        D: 'static,
        T: Reflect + TypePath + GetTypeRegistration;

    fn register_persisted<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration;

    fn track_persistence<T>(&mut self) -> &mut Self
    where
        T: Component;
}

impl PersistenceAppExt for App {
    fn register_persistence_type<D, T>(&mut self) -> &mut Self
    where
        D: 'static,
        T: Reflect + TypePath + GetTypeRegistration,
    {
        self.register_type::<T>()
            .register_type_data::<T, PersistenceDomain<D>>()
    }

    fn register_persisted<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration,
    {
        self.register_persistence_type::<WorkspacePersistence, T>()
            .track_persistence::<T>()
    }

    fn track_persistence<T>(&mut self) -> &mut Self
    where
        T: Component,
    {
        self.add_systems(Update, detect_change::<T>)
    }
}

type PersistedChanged<T> = Or<(Added<T>, Changed<T>)>;

fn detect_change<T: Component>(
    changed: Query<(), PersistedChanged<T>>,
    mut removed: RemovedComponents<T>,
    mut commands: Commands,
) {
    if !changed.is_empty() || removed.read().next().is_some() {
        commands.trigger(PersistenceDirty);
    }
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

    enum FeaturePersistence {}

    type FeaturePersisted = PersistenceDomain<FeaturePersistence>;

    #[test]
    fn feature_registration_builds_the_component_allowlist() {
        let mut app = App::new();
        app.register_persisted::<Saved>()
            .register_type::<Transient>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let filter = WorkspacePersisted::filter(&registry);

        assert!(filter.is_allowed::<Saved>());
        assert!(!filter.is_allowed::<Transient>());
    }

    #[test]
    fn named_persistence_domains_build_independent_allowlists() {
        let mut app = App::new();
        app.register_persistence_type::<FeaturePersistence, Saved>()
            .register_persisted::<Transient>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let feature = FeaturePersisted::filter(&registry);
        let workspace = WorkspacePersisted::filter(&registry);

        assert!(feature.is_allowed::<Saved>());
        assert!(!feature.is_allowed::<Transient>());
        assert!(!workspace.is_allowed::<Saved>());
        assert!(workspace.is_allowed::<Transient>());
    }
}
