use crate::Uuid;
pub use bevy_app::App;
use bevy_app::{AppExit, Last, Plugin, PostUpdate};
pub use bevy_ecs::prelude::With;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryFilter;
use bevy_ecs::reflect::AppTypeRegistry;
use bevy_ecs::resource::IsResource;
use bevy_ecs::system::SystemParam;
pub use bevy_reflect::TypeRegistry;
use bevy_reflect::{FromReflect, FromType, GetTypeRegistration, Reflect, TypePath};
pub use bevy_world_serialization::WorldFilter;
use bevy_world_serialization::{DynamicEntity, DynamicWorld};
pub use moonshine_save::prelude::Save;
use moonshine_save::prelude::SaveWorld;
use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use vmux_time::{
    BackoffPolicy, Clock, Deadline, DeadlinePlugin, DeadlineReached, DeadlineSet, Debounce,
    RetryBackoff, TimePlugin,
};
use web_time::Duration;

pub struct SchemaPlugin<S: Schema>(PhantomData<fn() -> S>);

impl<S: Schema> Default for SchemaPlugin<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S: Schema> Plugin for SchemaPlugin<S> {
    fn build(&self, app: &mut App) {
        app.init_resource::<AppTypeRegistry>();
        let filter = {
            let mut registry = app.world().resource::<AppTypeRegistry>().write();
            S::register(&mut registry);
            S::filter(&registry)
        };
        S::track(app);
        app.insert_resource(Types::<S> {
            filter,
            schema: PhantomData,
        });
    }
}

pub trait Schema: Send + Sync + Sized + 'static {
    fn register(registry: &mut TypeRegistry);

    fn filter(registry: &TypeRegistry) -> WorldFilter {
        StoreDomain::<Self>::filter(registry)
    }

    fn track(_app: &mut App) {}
}

#[derive(Resource)]
pub struct Types<S: Schema> {
    filter: WorldFilter,
    schema: PhantomData<fn() -> S>,
}

impl<S: Schema> Clone for Types<S> {
    fn clone(&self) -> Self {
        Self {
            filter: self.filter.clone(),
            schema: PhantomData,
        }
    }
}

impl<S: Schema> Types<S> {
    pub fn apply<F: QueryFilter>(&self, save: &mut SaveWorld<F>) {
        save.components = self.filter.clone();
        save.resources = self.filter.clone();
    }
}

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
    loaded: Option<crate::Revision>,
    dirty: Option<crate::Revision>,
    saving: Option<crate::Revision>,
    error: Option<String>,
}

impl State {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn loaded(&self) -> Option<u64> {
        self.loaded.map(|revision| revision.0)
    }

    pub fn dirty(&self) -> Option<u64> {
        self.dirty.map(|revision| revision.0)
    }

    pub fn saving(&self) -> Option<u64> {
        self.saving.map(|revision| revision.0)
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
        self.dirty = Some(revision.saturating_next());
    }

    pub fn begin(&mut self) -> Option<u64> {
        if self.phase != Phase::Ready || self.saving.is_some() {
            return None;
        }
        let revision = self.dirty.take()?;
        self.saving = Some(revision);
        Some(revision.0)
    }

    pub fn succeed(&mut self, revision: u64) -> bool {
        let revision = crate::Revision(revision);
        if self.saving != Some(revision) {
            return false;
        }
        self.loaded = Some(revision);
        self.saving = None;
        self.error = None;
        true
    }

    pub fn retry(&mut self, revision: u64, error: String) -> bool {
        let revision = crate::Revision(revision);
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

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceCommitRequest {
    pub request_id: Uuid,
    pub requester: Entity,
    pub profile: Entity,
    pub path: PathBuf,
    pub required: Entity,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceCommitCompleted {
    pub request_id: Uuid,
    pub requester: Entity,
    pub profile: Entity,
    pub store: Entity,
    pub path: PathBuf,
    pub revision: u64,
    pub result: Result<(), String>,
}

#[derive(Component)]
pub struct WorkspaceStoreValidator {
    pub name: &'static str,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub rejects: fn(&str) -> bool,
}

#[derive(Component)]
pub struct WorkspaceRestoreValidator {
    pub name: &'static str,
    #[allow(unknown_lints, vmux_ecs_ownership)]
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
                .is_some_and(|info| info.type_id() == TypeId::of::<T>())
        })
    }

    fn component<T: FromReflect>(&self) -> Result<Option<T>, String> {
        for component in &self.0.components {
            if component
                .get_represented_type_info()
                .is_none_or(|info| info.type_id() != TypeId::of::<T>())
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

pub struct AutoSavePlugin<S: Send + Sync + 'static>(PhantomData<fn() -> S>);

impl<S: Send + Sync + 'static> Default for AutoSavePlugin<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S: Send + Sync + 'static> Plugin for AutoSavePlugin<S> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimePlugin>() {
            app.add_plugins(TimePlugin);
        }
        if !app.is_plugin_added::<DeadlinePlugin<(DebounceTag, S)>>() {
            app.add_plugins(DeadlinePlugin::<(DebounceTag, S)>::default());
        }
        if !app.is_plugin_added::<DeadlinePlugin<(PeriodicTag, S)>>() {
            app.add_plugins(DeadlinePlugin::<(PeriodicTag, S)>::default());
        }
        if !app.is_plugin_added::<DeadlinePlugin<(RetryTag, S)>>() {
            app.add_plugins(DeadlinePlugin::<(RetryTag, S)>::default());
        }
        app.add_message::<AppExit>()
            .configure_sets(
                PostUpdate,
                (
                    AutoSaveSet::Track,
                    AutoSaveSet::Apply,
                    AutoSaveSet::Dispatch,
                )
                    .chain(),
            )
            .configure_sets(Last, AutoSaveSet::Flush.before(DeadlineSet::Sync))
            .add_observer(on_dirty::<S>)
            .add_observer(on_debounce::<S>)
            .add_observer(on_periodic::<S>)
            .add_observer(on_retry::<S>)
            .add_observer(on_result::<S>)
            .add_systems(PostUpdate, arm_dirty::<S>.in_set(AutoSaveSet::Apply))
            .add_systems(PostUpdate, dispatch::<S>.in_set(AutoSaveSet::Dispatch))
            .add_systems(Last, flush_on_exit::<S>.in_set(AutoSaveSet::Flush));
        if TypeId::of::<S>() == TypeId::of::<WorkspaceStore>() {
            app.add_observer(on_workspace_dirty::<S>);
        }
        #[cfg(all(host, feature = "host"))]
        app.configure_sets(Last, AutoSaveSet::Flush.before(crate::service::ShutdownSet));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AutoSaveSet {
    Track,
    Apply,
    Dispatch,
    Flush,
}

#[derive(Event, Debug)]
pub struct Dirty<S: Send + Sync + 'static>(PhantomData<fn() -> S>);

impl<S: Send + Sync + 'static> Default for Dirty<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S: Send + Sync + 'static> Clone for Dirty<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Send + Sync + 'static> Copy for Dirty<S> {}

impl<S: Send + Sync + 'static> Dirty<S> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct StorePath(pub PathBuf);

impl StorePath {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoSavePolicy {
    pub debounce: Duration,
    pub periodic: Option<Duration>,
    pub retry: BackoffPolicy,
}

impl Default for AutoSavePolicy {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(500),
            periodic: Some(Duration::from_secs(60)),
            retry: BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(60)),
        }
    }
}

impl AutoSavePolicy {
    pub const fn new(debounce: Duration, periodic: Option<Duration>, retry: BackoffPolicy) -> Self {
        Self {
            debounce,
            periodic,
            retry,
        }
    }

    pub const fn immediate() -> Self {
        Self {
            debounce: Duration::ZERO,
            periodic: None,
            retry: BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(60)),
        }
    }

    pub const fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self
    }

    pub const fn with_periodic(mut self, periodic: Option<Duration>) -> Self {
        self.periodic = periodic;
        self
    }

    pub const fn with_retry(mut self, retry: BackoffPolicy) -> Self {
        self.retry = retry;
        self
    }
}

#[derive(Component, Debug)]
#[require(State, AutoSavePolicy)]
pub struct AutoSave<S: Send + Sync + 'static>(PhantomData<fn() -> S>);

impl<S: Send + Sync + 'static> Default for AutoSave<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S: Send + Sync + 'static> Clone for AutoSave<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Send + Sync + 'static> Copy for AutoSave<S> {}

impl<S: Send + Sync + 'static> AutoSave<S> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct AutoSavePaused;

#[derive(EntityEvent, Clone, Debug, PartialEq, Eq)]
pub struct StoreWriteRequest<S: Send + Sync + 'static> {
    #[event_target]
    pub entity: Entity,
    pub path: PathBuf,
    pub revision: u64,
    pub flush: bool,
    marker: PhantomData<fn() -> S>,
}

impl<S: Send + Sync + 'static> StoreWriteRequest<S> {
    pub fn new(entity: Entity, path: PathBuf, revision: u64, flush: bool) -> Self {
        Self {
            entity,
            path,
            revision,
            flush,
            marker: PhantomData,
        }
    }
}

#[derive(EntityEvent, Clone, Debug, PartialEq, Eq)]
pub struct StoreWriteResult<S: Send + Sync + 'static> {
    #[event_target]
    pub entity: Entity,
    pub revision: u64,
    pub result: Result<(), String>,
    marker: PhantomData<fn() -> S>,
}

impl<S: Send + Sync + 'static> StoreWriteResult<S> {
    pub fn new(entity: Entity, revision: u64, result: Result<(), String>) -> Self {
        Self {
            entity,
            revision,
            result,
            marker: PhantomData,
        }
    }

    pub fn ok(entity: Entity, revision: u64) -> Self {
        Self::new(entity, revision, Ok(()))
    }

    pub fn err(entity: Entity, revision: u64, error: impl Into<String>) -> Self {
        Self::new(entity, revision, Err(error.into()))
    }
}

struct DebounceTag;

struct PeriodicTag;

struct RetryTag;

#[derive(Component)]
struct SaveDue<S: Send + Sync + 'static>(PhantomData<fn() -> S>);

impl<S: Send + Sync + 'static> Default for SaveDue<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

type DirtyStoreItem<'a, S> = (
    Entity,
    &'a mut State,
    &'a AutoSavePolicy,
    Has<AutoSavePaused>,
    Has<Deadline<(PeriodicTag, S)>>,
    Option<&'a RetryBackoff<(RetryTag, S)>>,
);

type ArmStoreItem<'a, S> = (
    Entity,
    &'a State,
    &'a AutoSavePolicy,
    Has<Debounce<(DebounceTag, S)>>,
    Has<Deadline<(PeriodicTag, S)>>,
    Has<SaveDue<S>>,
    Option<&'a RetryBackoff<(RetryTag, S)>>,
);

type DispatchStoreItem<'a, S> = (
    Entity,
    &'a mut State,
    &'a StorePath,
    Option<&'a RetryBackoff<(RetryTag, S)>>,
);

type ResultStoreItem<'a, S> = (
    &'a mut State,
    &'a AutoSavePolicy,
    Has<AutoSavePaused>,
    Option<&'a mut RetryBackoff<(RetryTag, S)>>,
);

type ActiveAutoSave<S> = (With<AutoSave<S>>, Without<AutoSavePaused>);

type DueAutoSave<S> = (With<AutoSave<S>>, With<SaveDue<S>>, Without<AutoSavePaused>);

#[derive(SystemParam)]
struct DirtyStores<'w, 's, S: Send + Sync + 'static> {
    clock: Res<'w, Clock>,
    stores: Query<'w, 's, DirtyStoreItem<'static, S>, With<AutoSave<S>>>,
    commands: Commands<'w, 's>,
}

impl<S: Send + Sync + 'static> DirtyStores<'_, '_, S> {
    fn mark(&mut self) {
        for (entity, mut state, policy, paused, has_periodic, retry) in &mut self.stores {
            state.advance();
            if let Some(retry) = retry {
                if retry.next_retry().is_some() {
                    continue;
                }
                if retry.exhausted() {
                    self.commands
                        .entity(entity)
                        .remove::<RetryBackoff<(RetryTag, S)>>();
                }
            }
            if paused || state.phase() != Phase::Ready || state.saving().is_some() {
                continue;
            }
            if policy.debounce.is_zero() {
                self.commands.entity(entity).insert(SaveDue::<S>::default());
                continue;
            }
            self.commands.entity(entity).remove::<SaveDue<S>>().insert(
                Debounce::<(DebounceTag, S)>::after(&self.clock, policy.debounce),
            );
            if let Some(interval) = policy.periodic
                && !has_periodic
            {
                self.commands
                    .entity(entity)
                    .insert(Deadline::<(PeriodicTag, S)>::after(&self.clock, interval));
            }
        }
    }
}

fn on_dirty<S: Send + Sync + 'static>(_: On<Dirty<S>>, mut stores: DirtyStores<S>) {
    stores.mark();
}

fn on_workspace_dirty<S: Send + Sync + 'static>(_: On<StoreDirty>, mut stores: DirtyStores<S>) {
    stores.mark();
}

fn on_debounce<S: Send + Sync + 'static>(
    reached: On<DeadlineReached<(DebounceTag, S)>>,
    stores: Query<(), With<AutoSave<S>>>,
    mut commands: Commands,
) {
    if stores.contains(reached.entity) {
        commands
            .entity(reached.entity)
            .insert(SaveDue::<S>::default());
    }
}

fn on_periodic<S: Send + Sync + 'static>(
    reached: On<DeadlineReached<(PeriodicTag, S)>>,
    stores: Query<(), With<AutoSave<S>>>,
    mut commands: Commands,
) {
    if stores.contains(reached.entity) {
        commands
            .entity(reached.entity)
            .insert(SaveDue::<S>::default());
    }
}

fn on_retry<S: Send + Sync + 'static>(
    reached: On<DeadlineReached<(RetryTag, S)>>,
    stores: Query<(), With<AutoSave<S>>>,
    mut commands: Commands,
) {
    if stores.contains(reached.entity) {
        commands
            .entity(reached.entity)
            .insert(SaveDue::<S>::default());
    }
}

fn arm_dirty<S: Send + Sync + 'static>(
    clock: Res<Clock>,
    stores: Query<ArmStoreItem<'_, S>, ActiveAutoSave<S>>,
    mut commands: Commands,
) {
    for (entity, state, policy, has_debounce, has_periodic, has_due, retry) in &stores {
        if !state.is_dirty() {
            if has_periodic {
                commands
                    .entity(entity)
                    .remove::<Deadline<(PeriodicTag, S)>>();
            }
            continue;
        }
        if state.phase() != Phase::Ready || state.saving().is_some() {
            continue;
        }
        if retry.is_some_and(|retry| retry.next_retry().is_some() || retry.exhausted()) {
            continue;
        }
        if has_due || has_debounce {
            if let Some(interval) = policy.periodic
                && !has_periodic
            {
                commands
                    .entity(entity)
                    .insert(Deadline::<(PeriodicTag, S)>::after(&clock, interval));
            }
            continue;
        }
        if policy.debounce.is_zero() {
            commands.entity(entity).insert(SaveDue::<S>::default());
            continue;
        }
        commands
            .entity(entity)
            .remove::<SaveDue<S>>()
            .insert(Debounce::<(DebounceTag, S)>::after(&clock, policy.debounce));
        if let Some(interval) = policy.periodic
            && !has_periodic
        {
            commands
                .entity(entity)
                .insert(Deadline::<(PeriodicTag, S)>::after(&clock, interval));
        }
    }
}

fn dispatch<S: Send + Sync + 'static>(
    mut stores: Query<DispatchStoreItem<'_, S>, DueAutoSave<S>>,
    mut commands: Commands,
) {
    for (entity, mut state, path, retry) in &mut stores {
        if !state.is_dirty() {
            commands.entity(entity).remove::<SaveDue<S>>();
            continue;
        }
        if state.phase() != Phase::Ready || state.saving().is_some() {
            continue;
        }
        if retry.is_some_and(|retry| retry.next_retry().is_some()) {
            commands.entity(entity).remove::<SaveDue<S>>();
            continue;
        }
        let Some(revision) = state.begin() else {
            continue;
        };
        commands.entity(entity).remove::<(
            SaveDue<S>,
            Debounce<(DebounceTag, S)>,
            Deadline<(PeriodicTag, S)>,
        )>();
        commands.trigger(StoreWriteRequest::<S>::new(
            entity,
            path.0.clone(),
            revision,
            false,
        ));
    }
}

fn on_result<S: Send + Sync + 'static>(
    result: On<StoreWriteResult<S>>,
    clock: Res<Clock>,
    mut stores: Query<ResultStoreItem<'_, S>, With<AutoSave<S>>>,
    mut commands: Commands,
) {
    let entity = result.entity;
    let Ok((mut state, policy, paused, retry)) = stores.get_mut(entity) else {
        return;
    };
    match &result.result {
        Ok(()) => {
            if !state.succeed(result.revision) {
                return;
            }
            commands
                .entity(entity)
                .remove::<RetryBackoff<(RetryTag, S)>>();
            if state.is_dirty() && !paused && state.phase() == Phase::Ready {
                if policy.debounce.is_zero() {
                    commands.entity(entity).insert(SaveDue::<S>::default());
                } else {
                    commands
                        .entity(entity)
                        .remove::<SaveDue<S>>()
                        .insert(Debounce::<(DebounceTag, S)>::after(&clock, policy.debounce));
                    if let Some(interval) = policy.periodic {
                        commands
                            .entity(entity)
                            .insert(Deadline::<(PeriodicTag, S)>::after(&clock, interval));
                    }
                }
            }
        }
        Err(error) => {
            if !state.retry(result.revision, error.clone()) {
                return;
            }
            commands.entity(entity).remove::<(
                SaveDue<S>,
                Debounce<(DebounceTag, S)>,
                Deadline<(PeriodicTag, S)>,
            )>();
            if let Some(mut retry) = retry {
                retry.record_failure(&clock);
            } else {
                commands
                    .entity(entity)
                    .insert(RetryBackoff::<(RetryTag, S)>::scheduled(
                        policy.retry,
                        &clock,
                    ));
            }
        }
    }
}

fn flush_on_exit<S: Send + Sync + 'static>(
    mut exits: MessageReader<AppExit>,
    mut stores: Query<(Entity, &mut State, &StorePath), ActiveAutoSave<S>>,
    mut commands: Commands,
) {
    if exits.read().next().is_none() {
        return;
    }
    for (entity, mut state, path) in &mut stores {
        if state.phase() != Phase::Ready || !state.is_dirty() || state.saving().is_some() {
            continue;
        }
        let Some(revision) = state.begin() else {
            continue;
        };
        commands.entity(entity).remove::<(
            SaveDue<S>,
            Debounce<(DebounceTag, S)>,
            Deadline<(PeriodicTag, S)>,
            RetryBackoff<(RetryTag, S)>,
        )>();
        commands.trigger(StoreWriteRequest::<S>::new(
            entity,
            path.0.clone(),
            revision,
            true,
        ));
    }
}

pub trait StoreAppExt {
    fn register_stored<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration;

    fn track_store<T>(&mut self) -> &mut Self
    where
        T: Component;

    fn track_domain<D, F, T>(&mut self) -> &mut Self
    where
        D: Send + Sync + 'static,
        F: QueryFilter + 'static,
        T: Component;
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TrackSet {
    Detect,
    Emit,
}

#[derive(Resource)]
struct TrackState<D: Send + Sync + 'static> {
    dirty: bool,
    marker: PhantomData<fn() -> D>,
}

impl<D: Send + Sync + 'static> Default for TrackState<D> {
    fn default() -> Self {
        Self {
            dirty: false,
            marker: PhantomData,
        }
    }
}

#[derive(Resource, Default)]
struct TrackedComponents(HashSet<(TypeId, TypeId, TypeId)>);

#[derive(Resource, Default)]
struct TrackedDomains(HashSet<TypeId>);

impl StoreAppExt for App {
    fn register_stored<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration,
    {
        self.register_type::<T>()
            .register_type_data::<T, StoreDomain<WorkspaceStore>>()
            .track_store::<T>()
    }

    fn track_store<T>(&mut self) -> &mut Self
    where
        T: Component,
    {
        self.track_domain::<WorkspaceStore, With<Save>, T>()
    }

    fn track_domain<D, F, T>(&mut self) -> &mut Self
    where
        D: Send + Sync + 'static,
        F: QueryFilter + 'static,
        T: Component,
    {
        let key = (TypeId::of::<D>(), TypeId::of::<F>(), TypeId::of::<T>());
        if !self
            .world_mut()
            .get_resource_or_init::<TrackedComponents>()
            .0
            .insert(key)
        {
            return self;
        }
        let domain_id = TypeId::of::<D>();
        if self
            .world_mut()
            .get_resource_or_init::<TrackedDomains>()
            .0
            .insert(domain_id)
        {
            self.init_resource::<TrackState<D>>();
            if domain_id == TypeId::of::<WorkspaceStore>() {
                self.world_mut().register_event_key::<StoreDirty>();
            } else {
                self.world_mut().register_event_key::<Dirty<D>>();
            }
            self.configure_sets(
                PostUpdate,
                (TrackSet::Detect, TrackSet::Emit)
                    .chain()
                    .in_set(AutoSaveSet::Track),
            )
            .add_systems(PostUpdate, emit_dirty::<D>.in_set(TrackSet::Emit));
        }
        self.add_systems(
            PostUpdate,
            detect_change::<D, F, T>.in_set(TrackSet::Detect),
        )
        .add_observer(detect_removal::<D, F, T>)
    }
}

type PersistedChanged<T> = Or<(Added<T>, Changed<T>)>;

type TrackedScope<F> = Or<(F, With<IsResource>)>;

fn detect_change<D: Send + Sync + 'static, F: QueryFilter + 'static, T: Component>(
    changed: Query<(), (TrackedScope<F>, PersistedChanged<T>)>,
    mut state: ResMut<TrackState<D>>,
) {
    if !changed.is_empty() {
        state.dirty = true;
    }
}

fn detect_removal<D: Send + Sync + 'static, F: QueryFilter + 'static, T: Component>(
    trigger: On<Remove, T>,
    scope: Query<(), TrackedScope<F>>,
    mut state: ResMut<TrackState<D>>,
) {
    if scope.contains(trigger.entity) {
        state.dirty = true;
    }
}

fn emit_dirty<D: Send + Sync + 'static>(mut state: ResMut<TrackState<D>>, mut commands: Commands) {
    if !state.dirty {
        return;
    }
    state.dirty = false;
    if TypeId::of::<D>() == TypeId::of::<WorkspaceStore>() {
        commands.trigger(StoreDirty);
    } else {
        commands.trigger(Dirty::<D>::default());
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
    fn shared_component_registration_does_not_duplicate_dirty_notifications() {
        let mut app = App::new();
        app.register_stored::<Saved>()
            .register_stored::<Saved>()
            .init_resource::<DirtyCount>()
            .add_observer(DirtyCount::record);
        app.world_mut().spawn((Save, Saved));
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 1);
    }

    #[crate::store(domain = WorkspaceStore, track; Saved)]
    struct SharedStore;

    #[test]
    fn tracked_schemas_preserve_workspace_membership_and_deduplicate_change_tracking() {
        let mut app = App::new();
        app.add_plugins(SchemaPlugin::<SharedStore>::default())
            .register_type::<Transient>()
            .init_resource::<DirtyCount>()
            .add_observer(DirtyCount::record);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let workspace = WorkspaceStored::filter(&registry);
        assert!(workspace.is_allowed::<Saved>());
        assert!(!workspace.is_allowed::<Transient>());
        assert!(!StoreDomain::<SharedStore>::filter(&registry).is_allowed::<Saved>());
        drop(registry);
        let entity = app.world_mut().spawn((Save, Saved)).id();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 1);
        app.register_stored::<Saved>();
        app.world_mut().entity_mut(entity).insert(Saved);
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 2);
        app.world_mut().entity_mut(entity).remove::<Saved>();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 3);
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

    #[crate::store(Saved)]
    struct FeatureStore;

    type FeatureStored = StoreDomain<FeatureStore>;

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
    fn store_schemas_build_independent_allowlists() {
        let mut app = App::new();
        app.add_plugins(SchemaPlugin::<FeatureStore>::default())
            .register_stored::<Transient>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let feature = FeatureStored::filter(&registry);
        let workspace = WorkspaceStored::filter(&registry);

        assert!(feature.is_allowed::<Saved>());
        assert!(!feature.is_allowed::<Transient>());
        assert!(!workspace.is_allowed::<Saved>());
        assert!(workspace.is_allowed::<Transient>());
    }

    #[derive(Component, Reflect, Default)]
    #[reflect(Component)]
    struct Note;

    #[derive(Component, Reflect, Default)]
    #[reflect(Component)]
    struct NoteBody(String);

    #[crate::store(filter = With<Note>, track; Note, NoteBody)]
    struct NoteStore;

    #[derive(Resource, Default)]
    struct RecordedWrites(Vec<(Entity, PathBuf, u64, bool)>);

    fn record_note_write(
        request: On<StoreWriteRequest<NoteStore>>,
        mut writes: ResMut<RecordedWrites>,
    ) {
        writes.0.push((
            request.entity,
            request.path.clone(),
            request.revision,
            request.flush,
        ));
    }

    #[test]
    fn autosave_tracks_macro_changes_debounces_and_flushes_periodically() {
        let clock = Clock::fake(1_700_000_000_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .init_resource::<RecordedWrites>()
            .init_resource::<DirtyCount>()
            .add_plugins((
                SchemaPlugin::<NoteStore>::default(),
                AutoSavePlugin::<NoteStore>::default(),
            ))
            .add_observer(record_note_write)
            .add_observer(DirtyCount::record);

        let store = app
            .world_mut()
            .spawn((
                AutoSave::<NoteStore>::default(),
                StorePath::new("/tmp/notes.ron"),
                AutoSavePolicy::new(
                    Duration::from_millis(500),
                    Some(Duration::from_secs(2)),
                    BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(8)),
                ),
            ))
            .id();

        let note = app.world_mut().spawn((Note, NoteBody("v1".into()))).id();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 0);
        assert!(app.world().get::<State>(store).unwrap().is_dirty());
        assert!(app.world().resource::<RecordedWrites>().0.is_empty());

        clock.advance(Duration::from_millis(300));
        app.world_mut().get_mut::<NoteBody>(note).unwrap().0 = "v2".into();
        app.update();
        assert!(app.world().resource::<RecordedWrites>().0.is_empty());

        clock.advance(Duration::from_millis(300));
        app.update();
        assert!(app.world().resource::<RecordedWrites>().0.is_empty());

        clock.advance(Duration::from_millis(200));
        app.update();
        let writes = &app.world().resource::<RecordedWrites>().0;
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, store);
        assert_eq!(writes[0].1, PathBuf::from("/tmp/notes.ron"));
        assert!(!writes[0].3);
        let first_rev = writes[0].2;

        app.world_mut().entity_mut(note).remove::<NoteBody>();
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 1);

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::ok(store, first_rev + 99));
        app.update();
        assert_eq!(
            app.world().get::<State>(store).unwrap().saving(),
            Some(first_rev)
        );

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::ok(store, first_rev));
        app.update();
        assert!(app.world().get::<State>(store).unwrap().is_dirty());

        clock.advance(Duration::from_millis(500));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 2);
        let second_rev = app.world().resource::<RecordedWrites>().0[1].2;
        assert!(second_rev > first_rev);
        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::ok(store, second_rev));
        app.update();
        assert!(!app.world().get::<State>(store).unwrap().is_dirty());

        app.world_mut().despawn(note);
        app.update();
        assert!(app.world().get::<State>(store).unwrap().is_dirty());
        for _ in 0..5 {
            clock.advance(Duration::from_millis(400));
            app.world_mut().trigger(Dirty::<NoteStore>::new());
            app.update();
        }
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 3);
    }

    #[test]
    fn autosave_retries_with_backoff_and_flushes_on_exit() {
        let clock = Clock::fake(1_700_000_000_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .init_resource::<RecordedWrites>()
            .add_plugins((
                SchemaPlugin::<NoteStore>::default(),
                AutoSavePlugin::<NoteStore>::default(),
            ))
            .add_observer(record_note_write);

        let store = app
            .world_mut()
            .spawn((
                AutoSave::<NoteStore>::default(),
                StorePath::new("/tmp/retry-notes.ron"),
                AutoSavePolicy::new(
                    Duration::from_millis(100),
                    None,
                    BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(4))
                        .with_max_attempts(2),
                ),
            ))
            .id();

        app.world_mut().spawn((Note, NoteBody("initial".into())));
        app.update();
        clock.advance(Duration::from_millis(100));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 1);
        let rev = app.world().resource::<RecordedWrites>().0[0].2;

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::err(store, rev, "disk full"));
        app.update();
        assert!(app.world().get::<State>(store).unwrap().is_dirty());
        assert_eq!(
            app.world().get::<State>(store).unwrap().error(),
            Some("disk full")
        );

        clock.advance(Duration::from_millis(500));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 1);

        clock.advance(Duration::from_millis(500));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 2);

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::err(store, rev, "still full"));
        app.update();

        clock.advance(Duration::from_secs(1));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 2);

        clock.advance(Duration::from_secs(1));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 3);

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::err(store, rev, "exhausted"));
        app.update();

        clock.advance(Duration::from_secs(10));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 3);

        app.world_mut().write_message(AppExit::Success);
        app.update();
        let writes = &app.world().resource::<RecordedWrites>().0;
        assert_eq!(writes.len(), 4);
        assert!(writes[3].3);
    }
}
