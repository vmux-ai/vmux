use std::any::TypeId;
use std::collections::HashSet;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use bevy_app::{App, AppExit, Last, Plugin, PostUpdate};
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryFilter;
use bevy_ecs::resource::IsResource;
use bevy_ecs::system::SystemParam;
use bevy_reflect::{GetTypeRegistration, Reflect, TypePath};
use moonshine_save::prelude::Save;
use vmux_time::{
    BackoffPolicy, Clock, Deadline, DeadlinePlugin, DeadlineReached, DeadlineSet, Debounce,
    RetryBackoff, TimePlugin,
};
use web_time::Duration;

use crate::schema::StoreDomain;

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
    fn register_domain<D, T>(&mut self) -> &mut Self
    where
        D: Send + Sync + 'static,
        T: Component + Reflect + TypePath + GetTypeRegistration;

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
    fn register_domain<D, T>(&mut self) -> &mut Self
    where
        D: Send + Sync + 'static,
        T: Component + Reflect + TypePath + GetTypeRegistration,
    {
        self.register_type::<T>()
            .register_type_data::<T, StoreDomain<D>>()
            .track_domain::<D, With<Save>, T>()
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
            self.world_mut().register_event_key::<Dirty<D>>();
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
    commands.trigger(Dirty::<D>::default());
}
