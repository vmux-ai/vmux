use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
#[cfg(test)]
use bevy_ecs::reflect::AppTypeRegistry;
use bevy_ecs::system::SystemParam;
use bevy_reflect::{FromReflect, FromType, GetTypeRegistration, Reflect, TypePath, TypeRegistry};
use bevy_world_serialization::{DynamicEntity, DynamicWorld, WorldFilter};
use moonshine_save::prelude::Save;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

pub struct StoreDomain<D>(PhantomData<fn() -> D>);

impl<D> Clone for StoreDomain<D> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<T, D: 'static> FromType<T> for StoreDomain<D> {
    fn from_type() -> Self {
        Self(PhantomData)
    }
}

impl<D: 'static> StoreDomain<D> {
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

pub enum WorkspaceStore {}

pub type WorkspaceStored = StoreDomain<WorkspaceStore>;

#[derive(Event, Clone, Copy, Debug, Default)]
pub struct StoreDirty;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    Materialize,
    Resolve,
    #[default]
    Ready,
    Failed,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    phase: Phase,
    loaded: Option<u64>,
    dirty: Option<u64>,
    saving: Option<u64>,
    error: Option<String>,
}

impl State {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn loaded(&self) -> Option<u64> {
        self.loaded
    }

    pub fn dirty(&self) -> Option<u64> {
        self.dirty
    }

    pub fn saving(&self) -> Option<u64> {
        self.saving
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.is_some()
    }

    pub fn advance(&mut self) {
        let revision = self
            .loaded
            .unwrap_or_default()
            .max(self.dirty.unwrap_or_default())
            .max(self.saving.unwrap_or_default());
        self.dirty = Some(revision.saturating_add(1));
    }

    pub fn begin(&mut self) -> Option<u64> {
        if self.phase != Phase::Ready || self.saving.is_some() {
            return None;
        }
        let revision = self.dirty.take()?;
        self.saving = Some(revision);
        Some(revision)
    }

    pub fn succeed(&mut self, revision: u64) -> bool {
        if self.saving != Some(revision) {
            return false;
        }
        self.loaded = Some(revision);
        self.saving = None;
        self.error = None;
        true
    }

    pub fn retry(&mut self, revision: u64, error: String) -> bool {
        if self.saving != Some(revision) {
            return false;
        }
        self.saving = None;
        self.dirty = Some(self.dirty.unwrap_or_default().max(revision));
        self.error = Some(error);
        true
    }

    pub fn materialize(&mut self) {
        self.phase = Phase::Materialize;
    }

    pub fn resolve(&mut self) {
        self.phase = Phase::Resolve;
    }

    pub fn ready(&mut self) {
        self.phase = Phase::Ready;
    }

    pub fn fail(&mut self, error: String) {
        self.phase = Phase::Failed;
        self.error = Some(error);
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

#[derive(Component)]
pub struct WorkspaceRestoreValidator {
    pub name: &'static str,
    pub validate: fn(&DynamicWorld) -> Result<(), String>,
}

impl WorkspaceRestoreValidator {
    pub fn unique<T>(name: &'static str) -> Self
    where
        T: Component + FromReflect + Eq + Hash,
    {
        Self {
            name,
            validate: Self::validate_unique::<T>,
        }
    }

    pub fn ownership<R: Component>() -> Self {
        Self {
            name: "ownership graph",
            validate: Self::validate_ownership::<R>,
        }
    }

    fn validate_unique<T>(snapshot: &DynamicWorld) -> Result<(), String>
    where
        T: Component + FromReflect + Eq + Hash,
    {
        let mut seen = HashSet::new();
        for entity in &snapshot.entities {
            let Some(value) = RestoreEntity(entity).component::<T>()? else {
                continue;
            };
            if !seen.insert(value) {
                return Err(format!("duplicate {}", std::any::type_name::<T>()));
            }
        }
        Ok(())
    }

    fn validate_ownership<R: Component>(snapshot: &DynamicWorld) -> Result<(), String> {
        let mut entities = HashSet::new();
        let mut parents = HashMap::new();
        for entity in &snapshot.entities {
            if !entities.insert(entity.entity) {
                return Err(format!("duplicate entity {}", entity.entity));
            }
            if let Some(parent) = RestoreEntity(entity).component::<ChildOf>()? {
                parents.insert(entity.entity, parent.parent());
            }
        }
        for entity in &snapshot.entities {
            let Some(parent) = parents.get(&entity.entity) else {
                continue;
            };
            if !entities.contains(parent) && !RestoreEntity(entity).contains::<R>() {
                return Err(format!("{} has missing owner {parent}", entity.entity));
            }
            let mut visited = HashSet::new();
            let mut current = entity.entity;
            while let Some(parent) = parents.get(&current) {
                if !visited.insert(current) {
                    return Err(format!("ownership cycle contains {current}"));
                }
                current = *parent;
            }
        }
        Ok(())
    }
}

struct RestoreEntity<'a>(&'a DynamicEntity);

impl RestoreEntity<'_> {
    fn contains<T: 'static>(&self) -> bool {
        self.0.components.iter().any(|component| {
            component
                .get_represented_type_info()
                .is_some_and(|info| info.type_id() == std::any::TypeId::of::<T>())
        })
    }

    fn component<T: FromReflect>(&self) -> Result<Option<T>, String> {
        for component in &self.0.components {
            if component
                .get_represented_type_info()
                .is_none_or(|info| info.type_id() != std::any::TypeId::of::<T>())
            {
                continue;
            }
            return T::from_reflect(component.as_partial_reflect())
                .map(Some)
                .ok_or_else(|| {
                    format!(
                        "invalid {} on {}",
                        std::any::type_name::<T>(),
                        self.0.entity
                    )
                });
        }
        Ok(None)
    }
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

pub trait StoreAppExt {
    fn register_store_type<D, T>(&mut self) -> &mut Self
    where
        D: 'static,
        T: Reflect + TypePath + GetTypeRegistration;

    fn register_stored<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration;

    fn track_store<T>(&mut self) -> &mut Self
    where
        T: Component;
}

impl StoreAppExt for App {
    fn register_store_type<D, T>(&mut self) -> &mut Self
    where
        D: 'static,
        T: Reflect + TypePath + GetTypeRegistration,
    {
        self.register_type::<T>()
            .register_type_data::<T, StoreDomain<D>>()
    }

    fn register_stored<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration,
    {
        self.register_store_type::<WorkspaceStore, T>()
            .track_store::<T>()
    }

    fn track_store<T>(&mut self) -> &mut Self
    where
        T: Component,
    {
        self.add_systems(Update, detect_change::<T>)
    }
}

type PersistedChanged<T> = Or<(Added<T>, Changed<T>)>;

fn detect_change<T: Component>(
    changed: Query<(), (With<Save>, PersistedChanged<T>)>,
    saved: Query<(), With<Save>>,
    mut removed: RemovedComponents<T>,
    mut commands: Commands,
) {
    let marker = std::any::TypeId::of::<T>() == std::any::TypeId::of::<Save>();
    let mut dirty = !changed.is_empty();
    for entity in removed.read() {
        if marker || saved.contains(entity) {
            dirty = true;
        }
    }
    if dirty {
        commands.trigger(StoreDirty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_completion_preserves_newer_edits_and_refuses_stale_results() {
        let mut state = State::default();
        state.advance();
        let first = state.begin().unwrap();
        state.advance();
        assert!(state.succeed(first));
        assert!(state.is_dirty());
        let second = state.begin().unwrap();
        assert!(!state.succeed(first));
        assert_eq!(state.saving(), Some(second));
        assert!(state.retry(second, "disk full".into()));
        assert_eq!(state.begin(), Some(second));
        assert!(state.succeed(second));
        assert_eq!(state.loaded(), Some(second));
        assert!(!state.is_dirty());
    }

    #[test]
    fn restoration_blocks_saves_until_runtime_resolution_commits() {
        let mut state = State::default();
        state.materialize();
        state.advance();
        assert_eq!(state.begin(), None);
        state.resolve();
        assert_eq!(state.begin(), None);
        state.ready();
        assert!(state.begin().is_some());
    }

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Saved;

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Transient;

    #[derive(Component, Reflect)]
    struct Root;

    #[test]
    fn restore_ownership_rejects_cycles_and_missing_owners_but_allows_runtime_roots() {
        let root = Entity::from_bits(1);
        let child = Entity::from_bits(2);
        let runtime = Entity::from_bits(3);
        let validator = WorkspaceRestoreValidator::ownership::<Root>();
        let mut snapshot = DynamicWorld {
            resources: Vec::new(),
            entities: vec![
                DynamicEntity {
                    entity: root,
                    components: vec![Box::new(Root), Box::new(ChildOf(runtime))],
                },
                DynamicEntity {
                    entity: child,
                    components: vec![Box::new(ChildOf(root))],
                },
            ],
        };
        assert!((validator.validate)(&snapshot).is_ok());
        snapshot.entities[1].components = vec![Box::new(ChildOf(runtime))];
        assert!(
            (validator.validate)(&snapshot)
                .unwrap_err()
                .contains("missing owner")
        );
        snapshot.entities[1].components = vec![Box::new(ChildOf(root))];
        snapshot.entities[0].components = vec![Box::new(Root), Box::new(ChildOf(child))];
        assert!(
            (validator.validate)(&snapshot)
                .unwrap_err()
                .contains("ownership cycle")
        );
    }

    #[derive(Resource, Default)]
    struct DirtyCount(u32);

    impl DirtyCount {
        fn record(_: On<StoreDirty>, mut count: ResMut<Self>) {
            count.0 += 1;
        }
    }

    #[test]
    fn runtime_component_updates_do_not_request_workspace_saves() {
        let mut app = App::new();
        app.register_stored::<Saved>()
            .track_store::<Save>()
            .init_resource::<DirtyCount>()
            .add_observer(DirtyCount::record);
        let entity = app.world_mut().spawn(Saved).id();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 0);
        app.world_mut().entity_mut(entity).insert(Saved);
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 0);
        app.world_mut().entity_mut(entity).remove::<Saved>();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 0);
        app.world_mut().entity_mut(entity).insert((Save, Saved));
        app.update();
        assert!(app.world().resource::<DirtyCount>().0 > 0);
        app.world_mut().resource_mut::<DirtyCount>().0 = 0;
        app.world_mut().entity_mut(entity).remove::<Saved>();
        app.update();
        assert!(app.world().resource::<DirtyCount>().0 > 0);
        app.world_mut().resource_mut::<DirtyCount>().0 = 0;
        app.world_mut().entity_mut(entity).despawn();
        app.update();
        assert!(app.world().resource::<DirtyCount>().0 > 0);
    }

    enum FeatureStore {}

    type FeaturePersisted = StoreDomain<FeatureStore>;

    #[test]
    fn feature_registration_builds_the_component_allowlist() {
        let mut app = App::new();
        app.register_stored::<Saved>().register_type::<Transient>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let filter = WorkspaceStored::filter(&registry);

        assert!(filter.is_allowed::<Saved>());
        assert!(!filter.is_allowed::<Transient>());
    }

    #[test]
    fn named_persistence_domains_build_independent_allowlists() {
        let mut app = App::new();
        app.register_store_type::<FeatureStore, Saved>()
            .register_stored::<Transient>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let feature = FeaturePersisted::filter(&registry);
        let workspace = WorkspaceStored::filter(&registry);

        assert!(feature.is_allowed::<Saved>());
        assert!(!feature.is_allowed::<Transient>());
        assert!(!workspace.is_allowed::<Saved>());
        assert!(workspace.is_allowed::<Transient>());
    }
}
