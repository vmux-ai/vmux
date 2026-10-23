use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
#[cfg(test)]
use bevy_ecs::reflect::AppTypeRegistry;
use bevy_ecs::system::SystemParam;
use bevy_reflect::{FromType, GetTypeRegistration, Reflect, TypePath, TypeRegistry};
use bevy_world_serialization::WorldFilter;
use std::marker::PhantomData;
use std::path::PathBuf;

use crate::{Revision, UnixMillis};

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

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PersistenceState {
    loaded: Option<Revision>,
    dirty: Option<Revision>,
    saving: Option<Revision>,
    last_saved_at: Option<UnixMillis>,
    last_error: Option<String>,
}

impl PersistenceState {
    pub const fn unloaded() -> Self {
        Self {
            loaded: None,
            dirty: None,
            saving: None,
            last_saved_at: None,
            last_error: None,
        }
    }

    pub const fn restored(revision: Revision) -> Self {
        Self {
            loaded: Some(revision),
            dirty: None,
            saving: None,
            last_saved_at: None,
            last_error: None,
        }
    }

    pub const fn loaded(&self) -> Option<Revision> {
        self.loaded
    }

    pub const fn dirty(&self) -> Option<Revision> {
        self.dirty
    }

    pub const fn saving(&self) -> Option<Revision> {
        self.saving
    }

    pub const fn last_saved_at(&self) -> Option<UnixMillis> {
        self.last_saved_at
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn mark_dirty(&mut self, revision: Revision) {
        self.dirty = Some(self.dirty.map_or(revision, |dirty| dirty.max(revision)));
    }

    pub fn start_save(&mut self) -> Option<Revision> {
        if self.saving.is_some() {
            return None;
        }
        let revision = self.dirty.take()?;
        self.saving = Some(revision);
        Some(revision)
    }

    pub fn save_succeeded(&mut self, revision: Revision, saved_at: UnixMillis) -> bool {
        if self.saving != Some(revision) {
            return false;
        }
        self.loaded = Some(revision);
        self.saving = None;
        self.last_saved_at = Some(saved_at);
        self.last_error = None;
        true
    }

    pub fn save_failed(&mut self, revision: Revision, error: impl Into<String>) -> bool {
        if self.saving != Some(revision) {
            return false;
        }
        self.saving = None;
        self.mark_dirty(revision);
        self.last_error = Some(error.into());
        true
    }
}

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

    #[derive(TypePath)]
    enum Issue {}

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

    #[test]
    fn typed_ids_can_be_registered_for_persistence() {
        let mut app = App::new();
        app.register_persisted::<vmux_api::Id<Issue>>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let filter = WorkspacePersisted::filter(&registry);

        assert!(filter.is_allowed::<vmux_api::Id<Issue>>());
    }

    #[test]
    fn mutation_during_save_remains_dirty_after_success() {
        let mut state = PersistenceState::restored(Revision::new(1));
        state.mark_dirty(Revision::new(2));

        assert_eq!(state.start_save(), Some(Revision::new(2)));
        state.mark_dirty(Revision::new(3));
        assert!(state.save_succeeded(Revision::new(2), UnixMillis(10)));

        assert_eq!(state.loaded(), Some(Revision::new(2)));
        assert_eq!(state.dirty(), Some(Revision::new(3)));
        assert_eq!(state.saving(), None);
        assert_eq!(state.last_saved_at(), Some(UnixMillis(10)));
    }

    #[test]
    fn failed_save_is_retryable() {
        let mut state = PersistenceState::restored(Revision::new(4));
        state.mark_dirty(Revision::new(5));
        assert_eq!(state.start_save(), Some(Revision::new(5)));

        assert!(state.save_failed(Revision::new(5), "disk full"));

        assert_eq!(state.dirty(), Some(Revision::new(5)));
        assert_eq!(state.start_save(), Some(Revision::new(5)));
        assert_eq!(state.last_error(), Some("disk full"));
    }

    #[test]
    fn stale_save_completion_cannot_replace_current_state() {
        let mut state = PersistenceState::restored(Revision::new(6));
        state.mark_dirty(Revision::new(7));
        assert_eq!(state.start_save(), Some(Revision::new(7)));

        assert!(!state.save_succeeded(Revision::new(6), UnixMillis(10)));

        assert_eq!(state.loaded(), Some(Revision::new(6)));
        assert_eq!(state.saving(), Some(Revision::new(7)));
    }
}
