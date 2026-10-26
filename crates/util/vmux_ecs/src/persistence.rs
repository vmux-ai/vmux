use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryFilter;
use bevy_ecs::reflect::AppTypeRegistry;
use bevy_ecs::system::SystemParam;
use bevy_reflect::{FromType, GetTypeRegistration, Reflect, TypePath, TypeRegistry};
use bevy_world_serialization::WorldFilter;
use moonshine_save::load::{DefaultUnloadFilter, LoadCommand, LoadStream, LoadWorld};
use moonshine_save::prelude::{SaveEvent, SaveOutput};
use moonshine_save::save::{DefaultSaveFilter, EntityFilter, SaveResult};
use moonshine_util::event::{OnSingle, SingleEvent, TriggerSingle};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::{Revision, UnixMillis};
use vmux_path::AtomicFile;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestorePhase {
    Discover,
    Read,
    Migrate,
    Parse,
    Validate,
    Materialize,
    Resolve,
    Ready,
    Failed,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkspaceRestoreSet {
    Validate,
    Resolve,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistenceRecovery {
    Healthy,
    HardReset(String),
    Failed(String),
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PersistenceState {
    schema_version: u32,
    phase: RestorePhase,
    store_present: bool,
    loaded: Option<Revision>,
    dirty: Option<Revision>,
    saving: Option<Revision>,
    last_saved_at: Option<UnixMillis>,
    recovery: PersistenceRecovery,
}

impl PersistenceState {
    pub const fn new(schema_version: u32) -> Self {
        Self {
            schema_version,
            phase: RestorePhase::Discover,
            store_present: false,
            loaded: None,
            dirty: None,
            saving: None,
            last_saved_at: None,
            recovery: PersistenceRecovery::Healthy,
        }
    }

    pub const fn restored(schema_version: u32, revision: Revision) -> Self {
        Self {
            schema_version,
            phase: RestorePhase::Ready,
            store_present: true,
            loaded: Some(revision),
            dirty: None,
            saving: None,
            last_saved_at: None,
            recovery: PersistenceRecovery::Healthy,
        }
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub const fn phase(&self) -> RestorePhase {
        self.phase
    }

    pub const fn store_present(&self) -> bool {
        self.store_present
    }

    pub const fn restore_complete(&self) -> bool {
        matches!(self.phase, RestorePhase::Ready | RestorePhase::Failed)
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

    pub fn current_revision(&self) -> Revision {
        self.dirty
            .into_iter()
            .chain(self.saving)
            .chain(self.loaded)
            .max()
            .unwrap_or_else(|| Revision::new(1))
    }

    pub const fn last_saved_at(&self) -> Option<UnixMillis> {
        self.last_saved_at
    }

    pub const fn recovery(&self) -> &PersistenceRecovery {
        &self.recovery
    }

    pub fn begin_read(&mut self) {
        self.phase = RestorePhase::Read;
    }

    pub fn begin_validation(&mut self, revision: Revision) {
        self.phase = RestorePhase::Validate;
        self.store_present = true;
        self.loaded = Some(revision);
    }

    pub fn begin_materialize(&mut self) {
        self.phase = RestorePhase::Materialize;
    }

    pub fn begin_resolve(&mut self) {
        self.phase = RestorePhase::Resolve;
    }

    pub fn ready(&mut self, store_present: bool) {
        self.phase = RestorePhase::Ready;
        self.store_present = store_present;
    }

    pub fn hard_reset(&mut self, reason: impl Into<String>) {
        self.phase = RestorePhase::Ready;
        self.store_present = false;
        self.loaded = None;
        self.recovery = PersistenceRecovery::HardReset(reason.into());
    }

    pub fn restore_failed(&mut self, error: impl Into<String>) {
        self.phase = RestorePhase::Failed;
        self.recovery = PersistenceRecovery::Failed(error.into());
    }

    pub fn mark_dirty(&mut self, revision: Revision) {
        self.dirty = Some(self.dirty.map_or(revision, |dirty| dirty.max(revision)));
    }

    pub fn advance_dirty(&mut self) -> Revision {
        let current = self
            .dirty
            .into_iter()
            .chain(self.saving)
            .chain(self.loaded)
            .max()
            .unwrap_or_else(|| Revision::new(0));
        let revision = current.next().unwrap_or(current);
        self.mark_dirty(revision);
        revision
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
        self.recovery = PersistenceRecovery::Healthy;
        true
    }

    pub fn save_failed(&mut self, revision: Revision, error: impl Into<String>) -> bool {
        if self.saving != Some(revision) {
            return false;
        }
        self.saving = None;
        self.mark_dirty(revision);
        self.recovery = PersistenceRecovery::Failed(error.into());
        true
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PageRestore;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WorkspaceStore;

pub struct TrackedLoadWorld<U: QueryFilter = DefaultUnloadFilter> {
    load: LoadWorld<U>,
    state: Entity,
}

impl<U: QueryFilter> TrackedLoadWorld<U> {
    pub fn from_stream(state: Entity, stream: impl LoadStream) -> Self {
        Self {
            load: LoadWorld::from_stream(stream),
            state,
        }
    }

    pub fn trigger(self, commands: &mut Commands)
    where
        U: Send + Sync + 'static,
    {
        commands.trigger_single(self);
    }
}

impl TrackedLoadWorld {
    pub fn default_from_stream(state: Entity, stream: impl LoadStream) -> Self {
        Self::from_stream(state, stream)
    }
}

impl<U> SingleEvent for TrackedLoadWorld<U> where U: QueryFilter + Send + Sync + 'static {}

pub fn load_tracked_on<U>(event: OnSingle<TrackedLoadWorld<U>>, mut commands: Commands)
where
    U: QueryFilter + Send + Sync + 'static,
{
    let tracked = event.consume().unwrap();
    commands.queue(move |world: &mut World| {
        let result = LoadCommand(tracked.load).apply(world);
        let Err(error) = result else {
            return;
        };
        if let Some(mut state) = world.get_mut::<PersistenceState>(tracked.state) {
            state.restore_failed(error.to_string());
        }
    });
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
    pub validate: fn(&mut World) -> Result<(), String>,
}

impl WorkspaceRestoreValidator {
    pub const fn new(name: &'static str, validate: fn(&mut World) -> Result<(), String>) -> Self {
        Self { name, validate }
    }

    pub fn unique<T>(name: &'static str) -> Self
    where
        T: Component + Clone + Eq + Hash,
    {
        Self::new(name, validate_unique::<T>)
    }

    pub const fn ownership() -> Self {
        Self::new("ownership graph", validate_ownership)
    }
}

fn validate_unique<T>(world: &mut World) -> Result<(), String>
where
    T: Component + Clone + Eq + Hash,
{
    let mut seen = HashSet::new();
    let mut values = world.query::<&T>();
    for value in values.iter(world) {
        if !seen.insert(value.clone()) {
            return Err(format!("duplicate {}", std::any::type_name::<T>()));
        }
    }
    Ok(())
}

fn validate_ownership(world: &mut World) -> Result<(), String> {
    let mut relationships = world.query::<(Entity, &ChildOf)>();
    let parents = relationships
        .iter(world)
        .map(|(entity, parent)| (entity, parent.parent()))
        .collect::<HashMap<_, _>>();
    for (entity, parent) in &parents {
        if world.get_entity(*parent).is_err() {
            return Err(format!("{entity} has missing owner {parent}"));
        }
        let mut visited = HashSet::new();
        let mut current = *entity;
        while let Some(parent) = parents.get(&current) {
            if !visited.insert(current) {
                return Err(format!("ownership cycle contains {current}"));
            }
            current = *parent;
        }
    }
    Ok(())
}

#[derive(SystemParam)]
pub struct WorkspaceStoreValidators<'w, 's> {
    validators: Query<'w, 's, &'static WorkspaceStoreValidator>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct StoreMetadata {
    name: String,
    schema_version: u32,
    revision: Revision,
    saved_at: UnixMillis,
}

impl StoreMetadata {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub const fn revision(&self) -> Revision {
        self.revision
    }

    pub const fn saved_at(&self) -> UnixMillis {
        self.saved_at
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct StoreDocument {
    metadata: StoreMetadata,
    payload: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreSnapshot {
    metadata: StoreMetadata,
    payload: String,
}

impl StoreSnapshot {
    pub const fn metadata(&self) -> &StoreMetadata {
        &self.metadata
    }

    pub fn payload(&self) -> &str {
        &self.payload
    }

    pub fn into_payload(self) -> String {
        self.payload
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreReset {
    old_schema_version: Option<u32>,
    reason: String,
}

impl StoreReset {
    pub const fn old_schema_version(&self) -> Option<u32> {
        self.old_schema_version
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreLoad {
    Missing,
    Ready(StoreSnapshot),
    Reset(StoreReset),
}

#[derive(Clone, Debug)]
pub struct DurableStore {
    path: PathBuf,
    name: &'static str,
    schema_version: u32,
}

impl DurableStore {
    pub fn new(path: impl Into<PathBuf>, name: &'static str, schema_version: u32) -> Self {
        Self {
            path: path.into(),
            name,
            schema_version,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn load(&self) -> std::io::Result<StoreLoad> {
        let body = match std::fs::read_to_string(&self.path) {
            Ok(body) => body,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StoreLoad::Missing);
            }
            Err(error) => return Err(error),
        };
        let document = match ron::from_str::<StoreDocument>(&body) {
            Ok(document) => document,
            Err(error) => {
                return self.reset(None, format!("invalid store document: {error}"));
            }
        };
        if document.metadata.name != self.name {
            return self.reset(
                Some(document.metadata.schema_version),
                format!(
                    "store name mismatch: expected {}, found {}",
                    self.name, document.metadata.name
                ),
            );
        }
        if document.metadata.schema_version != self.schema_version {
            return self.reset(
                Some(document.metadata.schema_version),
                format!(
                    "schema mismatch: expected {}, found {}",
                    self.schema_version, document.metadata.schema_version
                ),
            );
        }
        Ok(StoreLoad::Ready(StoreSnapshot {
            metadata: document.metadata,
            payload: document.payload,
        }))
    }

    pub fn write(&self, revision: Revision, payload: String) -> std::io::Result<UnixMillis> {
        let saved_at = UnixMillis::now();
        let document = StoreDocument {
            metadata: StoreMetadata {
                name: self.name.to_string(),
                schema_version: self.schema_version,
                revision,
                saved_at,
            },
            payload,
        };
        let body = ron::ser::to_string_pretty(&document, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        AtomicFile::write(&self.path, body.as_bytes())?;
        Ok(saved_at)
    }

    pub fn hard_reset(&self, reason: impl Into<String>) -> std::io::Result<StoreReset> {
        self.reset(None, reason.into()).map(|load| match load {
            StoreLoad::Reset(reset) => reset,
            StoreLoad::Missing | StoreLoad::Ready(_) => unreachable!(),
        })
    }

    fn reset(&self, old_schema_version: Option<u32>, reason: String) -> std::io::Result<StoreLoad> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(StoreLoad::Reset(StoreReset {
            old_schema_version,
            reason,
        }))
    }
}

pub struct AtomicSaveWorld<F: QueryFilter = DefaultSaveFilter> {
    pub entities: EntityFilter,
    pub resources: WorldFilter,
    pub components: WorldFilter,
    store: DurableStore,
    state: Option<Entity>,
    revision: Revision,
    filter: PhantomData<F>,
}

impl<F: QueryFilter> AtomicSaveWorld<F> {
    pub fn new(store: DurableStore, state: Entity, revision: Revision) -> Self {
        Self::with_state(store, Some(state), revision)
    }

    pub fn untracked(store: DurableStore, revision: Revision) -> Self {
        Self::with_state(store, None, revision)
    }

    fn with_state(store: DurableStore, state: Option<Entity>, revision: Revision) -> Self {
        Self {
            entities: EntityFilter::allow_all(),
            resources: WorldFilter::deny_all(),
            components: WorldFilter::allow_all(),
            store,
            state,
            revision,
            filter: PhantomData,
        }
    }
}

impl<F> SingleEvent for AtomicSaveWorld<F> where F: QueryFilter + Send + Sync + 'static {}

impl<F> SaveEvent for AtomicSaveWorld<F>
where
    F: QueryFilter + Send + Sync + 'static,
{
    type SaveFilter = F;

    fn filter_entity(&self, entity: EntityRef) -> bool {
        match &self.entities {
            EntityFilter::Allow(allowed) => allowed.contains(&entity.id()),
            EntityFilter::Block(blocked) => !blocked.contains(&entity.id()),
        }
    }

    fn component_filter(&mut self) -> WorldFilter {
        std::mem::replace(&mut self.components, WorldFilter::Unset)
    }

    fn resource_filter(&mut self) -> WorldFilter {
        std::mem::replace(&mut self.resources, WorldFilter::Unset)
    }

    fn after_save(&mut self, world: &mut World, result: &SaveResult) {
        let write = match result {
            Ok(saved) => {
                let payload = {
                    let registry = world.resource::<AppTypeRegistry>().read();
                    saved.world.serialize(&registry)
                };
                match payload {
                    Ok(payload) => self.store.write(self.revision, payload),
                    Err(error) => Err(std::io::Error::other(error)),
                }
            }
            Err(error) => Err(std::io::Error::other(error.to_string())),
        };
        let Some(state) = self.state else {
            return;
        };
        let Some(mut state) = world.get_mut::<PersistenceState>(state) else {
            return;
        };
        match write {
            Ok(saved_at) => {
                state.save_succeeded(self.revision, saved_at);
            }
            Err(error) => {
                state.save_failed(self.revision, error.to_string());
            }
        }
    }

    fn output(&mut self) -> SaveOutput {
        SaveOutput::Drop
    }
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
        let mut state = PersistenceState::restored(1, Revision::new(1));
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
        let mut state = PersistenceState::restored(1, Revision::new(4));
        state.mark_dirty(Revision::new(5));
        assert_eq!(state.start_save(), Some(Revision::new(5)));

        assert!(state.save_failed(Revision::new(5), "disk full"));

        assert_eq!(state.dirty(), Some(Revision::new(5)));
        assert_eq!(state.start_save(), Some(Revision::new(5)));
        assert_eq!(
            state.recovery(),
            &PersistenceRecovery::Failed("disk full".to_string())
        );
    }

    #[test]
    fn stale_save_completion_cannot_replace_current_state() {
        let mut state = PersistenceState::restored(1, Revision::new(6));
        state.mark_dirty(Revision::new(7));
        assert_eq!(state.start_save(), Some(Revision::new(7)));

        assert!(!state.save_succeeded(Revision::new(6), UnixMillis(10)));

        assert_eq!(state.loaded(), Some(Revision::new(6)));
        assert_eq!(state.saving(), Some(Revision::new(7)));
    }

    #[test]
    fn durable_store_round_trips_embedded_metadata_and_payload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.ron");
        let store = DurableStore::new(&path, "workspace", 4);

        let saved_at = store
            .write(Revision::new(9), "(entities: {})".to_string())
            .unwrap();
        let StoreLoad::Ready(snapshot) = store.load().unwrap() else {
            panic!("store should be ready");
        };

        assert_eq!(snapshot.metadata().name(), "workspace");
        assert_eq!(snapshot.metadata().schema_version(), 4);
        assert_eq!(snapshot.metadata().revision(), Revision::new(9));
        assert_eq!(snapshot.metadata().saved_at(), saved_at);
        assert_eq!(snapshot.payload(), "(entities: {})");
    }

    #[test]
    fn incompatible_schema_hard_resets_only_the_store_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.ron");
        let sibling = directory.path().join("credentials.bin");
        DurableStore::new(&path, "workspace", 3)
            .write(Revision::new(1), "old".to_string())
            .unwrap();
        std::fs::write(&sibling, b"secret").unwrap();

        let load = DurableStore::new(&path, "workspace", 4).load().unwrap();
        let StoreLoad::Reset(reset) = load else {
            panic!("store should reset");
        };

        assert_eq!(reset.old_schema_version(), Some(3));
        assert!(!path.exists());
        assert_eq!(std::fs::read(sibling).unwrap(), b"secret");
    }

    #[test]
    fn corrupted_store_is_detected_and_reset() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.ron");
        std::fs::write(&path, "truncated: (").unwrap();

        let load = DurableStore::new(&path, "workspace", 4).load().unwrap();
        let StoreLoad::Reset(reset) = load else {
            panic!("store should reset");
        };

        assert!(reset.reason().contains("invalid store document"));
        assert!(!path.exists());
    }

    #[test]
    fn materialization_failure_becomes_typed_persistence_state() {
        let mut app = App::new();
        app.add_plugins((bevy::MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .add_observer(load_tracked_on::<DefaultUnloadFilter>);
        let state = app.world_mut().spawn(PersistenceState::new(1)).id();
        app.world_mut()
            .trigger_single(TrackedLoadWorld::default_from_stream(
                state,
                std::io::Cursor::new(b"not a world".to_vec()),
            ));

        app.update();

        let state = app.world().get::<PersistenceState>(state).unwrap();
        assert_eq!(state.phase(), RestorePhase::Failed);
        assert!(matches!(state.recovery(), PersistenceRecovery::Failed(_)));
    }

    #[test]
    fn restore_validator_rejects_duplicate_stable_identity() {
        enum Record {}

        let mut app = App::new();
        app.world_mut()
            .spawn(vmux_api::Id::<Record>::new("record-1"));
        app.world_mut()
            .spawn(vmux_api::Id::<Record>::new("record-1"));
        let validator = WorkspaceRestoreValidator::unique::<vmux_api::Id<Record>>("record IDs");

        assert!((validator.validate)(app.world_mut()).is_err());
    }
}
