use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::marker::PhantomData;
use std::sync::{Arc, Condvar, Mutex};
use std::task::Waker;

use bevy_app::{App, First, Last, Plugin};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use web_time::{Duration, Instant};

use crate::{Clock, TimePlugin};

const COALESCE_WINDOW: Duration = Duration::from_millis(1);

pub struct DeadlinePlugin<T: Send + Sync + 'static>(PhantomData<fn() -> T>);

impl<T: Send + Sync + 'static> Default for DeadlinePlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync + 'static> Plugin for DeadlinePlugin<T> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimePlugin>() {
            app.add_plugins(TimePlugin);
        }
        app.add_systems(First, fire::<T>.in_set(DeadlineSet::Fire))
            .add_systems(Last, sync::<T>.in_set(DeadlineSet::Sync));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeadlineSet {
    Wake,
    Fire,
    Sync,
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct DeadlineReached<T: Send + Sync + 'static> {
    #[event_target]
    pub entity: Entity,
    marker: PhantomData<fn() -> T>,
}

impl<T: Send + Sync + 'static> DeadlineReached<T> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            marker: PhantomData,
        }
    }
}

#[derive(Component, Debug)]
#[component(on_insert = register::<T>, on_discard = unregister::<T>)]
pub struct Deadline<T: Send + Sync + 'static> {
    at: Instant,
    marker: PhantomData<fn() -> T>,
}

impl<T: Send + Sync + 'static> Clone for Deadline<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Deadline<T> {}

impl<T: Send + Sync + 'static> PartialEq for Deadline<T> {
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at
    }
}

impl<T: Send + Sync + 'static> Eq for Deadline<T> {}

impl<T: Send + Sync + 'static> Deadline<T> {
    pub fn at(at: Instant) -> Self {
        Self {
            at,
            marker: PhantomData,
        }
    }

    pub fn after(clock: &Clock, duration: Duration) -> Self {
        Self::at(clock.deadline(duration))
    }

    pub fn instant(&self) -> Instant {
        self.at
    }

    pub fn set(&mut self, at: Instant) {
        self.at = at;
    }

    pub fn is_due(&self, clock: &Clock) -> bool {
        clock.now() >= self.at
    }
}

#[derive(Component, Debug)]
#[component(on_insert = register_debounce::<T>, on_discard = unregister_debounce::<T>)]
pub struct Debounce<T: Send + Sync + 'static> {
    window: Duration,
    due_at: Option<Instant>,
    marker: PhantomData<fn() -> T>,
}

impl<T: Send + Sync + 'static> Clone for Debounce<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Debounce<T> {}

impl<T: Send + Sync + 'static> Debounce<T> {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            due_at: None,
            marker: PhantomData,
        }
    }

    pub fn after(clock: &Clock, window: Duration) -> Self {
        Self {
            window,
            due_at: Some(clock.deadline(window)),
            marker: PhantomData,
        }
    }

    pub fn window(&self) -> Duration {
        self.window
    }

    pub fn due_at(&self) -> Option<Instant> {
        self.due_at
    }

    pub fn reset(&mut self, clock: &Clock) {
        self.due_at = Some(clock.deadline(self.window));
    }

    pub fn is_due(&self, clock: &Clock) -> bool {
        self.due_at.is_some_and(|at| clock.now() >= at)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackoffPolicy {
    pub initial: Duration,
    pub max: Duration,
    pub max_attempts: Option<u32>,
}

impl BackoffPolicy {
    pub const fn exponential(initial: Duration, max: Duration) -> Self {
        Self {
            initial,
            max,
            max_attempts: None,
        }
    }

    pub const fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = Some(max_attempts);
        self
    }

    pub fn delay_for(&self, attempt: u32) -> Duration {
        if self.initial.is_zero() {
            return Duration::ZERO;
        }
        let cap = self.max.max(self.initial);
        let mut delay = self.initial;
        for _ in 0..attempt {
            if delay >= cap {
                return cap;
            }
            delay = delay.saturating_mul(2).min(cap);
        }
        delay
    }
}

#[derive(Component, Debug)]
#[component(on_insert = register_retry::<T>, on_discard = unregister_retry::<T>)]
pub struct RetryBackoff<T: Send + Sync + 'static> {
    policy: BackoffPolicy,
    attempts: u32,
    next_retry: Option<Instant>,
    marker: PhantomData<fn() -> T>,
}

impl<T: Send + Sync + 'static> Clone for RetryBackoff<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for RetryBackoff<T> {}

impl<T: Send + Sync + 'static> RetryBackoff<T> {
    pub fn new(policy: BackoffPolicy) -> Self {
        Self {
            policy,
            attempts: 0,
            next_retry: None,
            marker: PhantomData,
        }
    }

    pub fn exponential(initial: Duration, max: Duration) -> Self {
        Self::new(BackoffPolicy::exponential(initial, max))
    }

    pub fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.policy = self.policy.with_max_attempts(max_attempts);
        self
    }

    pub fn scheduled(policy: BackoffPolicy, clock: &Clock) -> Self {
        let mut backoff = Self::new(policy);
        let _ = backoff.record_failure(clock);
        backoff
    }

    pub fn policy(&self) -> BackoffPolicy {
        self.policy
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn exhausted(&self) -> bool {
        self.policy
            .max_attempts
            .is_some_and(|limit| self.attempts >= limit)
    }

    pub fn next_retry(&self) -> Option<Instant> {
        self.next_retry
    }

    pub fn next_delay(&self) -> Duration {
        self.policy.delay_for(self.attempts)
    }

    pub fn record_failure(&mut self, clock: &Clock) -> Option<Instant> {
        if self.exhausted() {
            self.next_retry = None;
            return None;
        }
        let delay = self.policy.delay_for(self.attempts);
        self.attempts = self.attempts.saturating_add(1);
        let at = clock.deadline(delay);
        self.next_retry = Some(at);
        Some(at)
    }

    pub fn reset(&mut self) {
        self.attempts = 0;
        self.next_retry = None;
    }

    pub fn is_due(&self, clock: &Clock) -> bool {
        self.next_retry.is_some_and(|at| clock.now() >= at)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Slot {
    Deadline,
    Debounce,
    Retry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Key {
    entity: Entity,
    kind: TypeId,
    slot: Slot,
}

impl Key {
    fn deadline<T: Send + Sync + 'static>(entity: Entity) -> Self {
        Self {
            entity,
            kind: TypeId::of::<T>(),
            slot: Slot::Deadline,
        }
    }

    fn debounce<T: Send + Sync + 'static>(entity: Entity) -> Self {
        Self {
            entity,
            kind: TypeId::of::<T>(),
            slot: Slot::Debounce,
        }
    }

    fn retry<T: Send + Sync + 'static>(entity: Entity) -> Self {
        Self {
            entity,
            kind: TypeId::of::<T>(),
            slot: Slot::Retry,
        }
    }
}

#[derive(Resource, Clone, Default)]
pub struct DeadlineScheduler(Arc<WakeDriver>);

#[derive(Default)]
struct WakeDriver {
    shared: Arc<(Mutex<ScheduleState>, Condvar)>,
}

#[derive(Default)]
struct ScheduleState {
    deadlines: BTreeMap<Instant, BTreeSet<Key>>,
    owners: HashMap<Key, Instant>,
    waker: Option<Waker>,
    #[cfg(not(target_family = "wasm"))]
    worker: Option<std::thread::JoinHandle<()>>,
    stopped: bool,
}

impl ScheduleState {
    fn remove(&mut self, key: Key) {
        let Some(at) = self.owners.remove(&key) else {
            return;
        };
        let Some(entries) = self.deadlines.get_mut(&at) else {
            return;
        };
        entries.remove(&key);
        if entries.is_empty() {
            self.deadlines.remove(&at);
        }
    }
}

impl DeadlineScheduler {
    pub fn pending_count(&self) -> usize {
        self.0
            .shared
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .owners
            .len()
    }

    pub fn earliest(&self) -> Option<Instant> {
        self.0
            .shared
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .deadlines
            .keys()
            .next()
            .copied()
    }

    pub(crate) fn has_waker(&self) -> bool {
        self.0
            .shared
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .waker
            .is_some()
    }

    pub(crate) fn set_waker(&self, waker: Option<Waker>, allow_worker: bool) {
        let (mutex, wake) = &*self.0.shared;
        let mut state = mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.waker = waker;
        let has_work = state.waker.is_some() && !state.deadlines.is_empty();
        if has_work {
            wake.notify_one();
        }
        if allow_worker && has_work && !state.stopped {
            WakeDriver::ensure_worker(&self.0.shared, &mut state);
        }
    }

    fn schedule(&self, key: Key, at: Instant, allow_worker: bool) {
        let (mutex, wake) = &*self.0.shared;
        let mut state = mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.stopped {
            return;
        }
        if state.owners.get(&key) == Some(&at) {
            return;
        }
        let earliest = state.deadlines.keys().next().copied();
        state.remove(key);
        state.deadlines.entry(at).or_default().insert(key);
        state.owners.insert(key, at);
        if earliest.is_none_or(|earliest| at < earliest) {
            wake.notify_one();
        }
        if allow_worker && state.waker.is_some() {
            WakeDriver::ensure_worker(&self.0.shared, &mut state);
        }
    }

    fn cancel(&self, key: Key) {
        let (state, wake) = &*self.0.shared;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let earliest = state.deadlines.keys().next().copied();
        state.remove(key);
        if state.deadlines.keys().next().copied() != earliest {
            wake.notify_one();
        }
    }
}

impl WakeDriver {
    #[cfg(not(target_family = "wasm"))]
    fn ensure_worker(shared: &Arc<(Mutex<ScheduleState>, Condvar)>, state: &mut ScheduleState) {
        if state.worker.is_some() {
            return;
        }
        let shared = Arc::clone(shared);
        if let Ok(handle) = std::thread::Builder::new()
            .name("vmux-time-wake".to_string())
            .spawn(move || Self::run(shared))
        {
            state.worker = Some(handle);
        }
    }

    #[cfg(target_family = "wasm")]
    fn ensure_worker(_shared: &Arc<(Mutex<ScheduleState>, Condvar)>, _state: &mut ScheduleState) {}

    #[cfg(not(target_family = "wasm"))]
    fn run(shared: Arc<(Mutex<ScheduleState>, Condvar)>) {
        let (mutex, wake) = &*shared;
        let mut state = mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if state.stopped {
                return;
            }
            let Some(deadline) = state.deadlines.keys().next().copied() else {
                state = wake
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                continue;
            };
            let wake_at = deadline + COALESCE_WINDOW;
            let delay = wake_at.saturating_duration_since(Instant::now());
            if !delay.is_zero() {
                let (next, timeout) = wake
                    .wait_timeout(state, delay)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state = next;
                if state.stopped {
                    return;
                }
                if !timeout.timed_out() || state.deadlines.keys().next().copied() != Some(deadline)
                {
                    continue;
                }
            }
            let now = Instant::now();
            while state
                .deadlines
                .first_key_value()
                .is_some_and(|(at, _)| *at <= now)
            {
                let Some((at, keys)) = state.deadlines.pop_first() else {
                    break;
                };
                for key in keys {
                    if state.owners.get(&key) == Some(&at) {
                        state.owners.remove(&key);
                    }
                }
            }
            let waker = state.waker.clone();
            drop(state);
            if let Some(waker) = waker {
                waker.wake();
            }
            state = mutex
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

impl Drop for WakeDriver {
    fn drop(&mut self) {
        let (mutex, wake) = &*self.shared;
        #[cfg(not(target_family = "wasm"))]
        let worker = {
            let mut state = mutex
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.stopped = true;
            state.deadlines.clear();
            state.owners.clear();
            state.waker = None;
            state.worker.take()
        };
        #[cfg(target_family = "wasm")]
        {
            let mut state = mutex
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.stopped = true;
            state.deadlines.clear();
            state.owners.clear();
            state.waker = None;
        }
        wake.notify_all();
        #[cfg(not(target_family = "wasm"))]
        if let Some(worker) = worker {
            let _ = worker.join();
        }
    }
}

fn register<T: Send + Sync + 'static>(world: DeferredWorld, context: HookContext) {
    let Some(deadline) = world.get::<Deadline<T>>(context.entity) else {
        return;
    };
    let at = deadline.at;
    let is_fake = world.get_resource::<Clock>().is_some_and(Clock::is_fake);
    if let Some(scheduler) = world.get_resource::<DeadlineScheduler>() {
        scheduler.schedule(Key::deadline::<T>(context.entity), at, !is_fake);
    }
}

fn unregister<T: Send + Sync + 'static>(world: DeferredWorld, context: HookContext) {
    if let Some(scheduler) = world.get_resource::<DeadlineScheduler>() {
        scheduler.cancel(Key::deadline::<T>(context.entity));
    }
}

fn register_debounce<T: Send + Sync + 'static>(mut world: DeferredWorld, context: HookContext) {
    let now = world
        .get_resource::<Clock>()
        .map_or_else(Instant::now, Clock::now);
    let is_fake = world.get_resource::<Clock>().is_some_and(Clock::is_fake);
    let Some(mut debounce) = world.get_mut::<Debounce<T>>(context.entity) else {
        return;
    };
    let at = match debounce.due_at {
        Some(at) => at,
        None => {
            let at = now + debounce.window;
            debounce.due_at = Some(at);
            at
        }
    };
    if let Some(scheduler) = world.get_resource::<DeadlineScheduler>() {
        scheduler.schedule(Key::debounce::<T>(context.entity), at, !is_fake);
    }
}

fn unregister_debounce<T: Send + Sync + 'static>(world: DeferredWorld, context: HookContext) {
    if let Some(scheduler) = world.get_resource::<DeadlineScheduler>() {
        scheduler.cancel(Key::debounce::<T>(context.entity));
    }
}

fn register_retry<T: Send + Sync + 'static>(world: DeferredWorld, context: HookContext) {
    let Some(retry) = world.get::<RetryBackoff<T>>(context.entity) else {
        return;
    };
    let next = retry.next_retry;
    let is_fake = world.get_resource::<Clock>().is_some_and(Clock::is_fake);
    if let Some(scheduler) = world.get_resource::<DeadlineScheduler>() {
        match next {
            Some(at) => scheduler.schedule(Key::retry::<T>(context.entity), at, !is_fake),
            None => scheduler.cancel(Key::retry::<T>(context.entity)),
        }
    }
}

fn unregister_retry<T: Send + Sync + 'static>(world: DeferredWorld, context: HookContext) {
    if let Some(scheduler) = world.get_resource::<DeadlineScheduler>() {
        scheduler.cancel(Key::retry::<T>(context.entity));
    }
}

fn fire<T: Send + Sync + 'static>(
    clock: Res<Clock>,
    scheduler: Res<DeadlineScheduler>,
    deadlines: Query<(Entity, &Deadline<T>)>,
    debounces: Query<(Entity, &Debounce<T>)>,
    mut retries: Query<(Entity, &mut RetryBackoff<T>)>,
    mut commands: Commands,
) {
    let now = clock.now();
    for (entity, deadline) in &deadlines {
        if now < deadline.at {
            continue;
        }
        scheduler.cancel(Key::deadline::<T>(entity));
        commands
            .entity(entity)
            .remove::<Deadline<T>>()
            .trigger(DeadlineReached::<T>::new);
    }
    for (entity, debounce) in &debounces {
        if debounce.due_at.is_none_or(|at| now < at) {
            continue;
        }
        scheduler.cancel(Key::debounce::<T>(entity));
        commands
            .entity(entity)
            .remove::<Debounce<T>>()
            .trigger(DeadlineReached::<T>::new);
    }
    for (entity, mut retry) in &mut retries {
        if retry.next_retry.is_none_or(|at| now < at) {
            continue;
        }
        retry.next_retry = None;
        scheduler.cancel(Key::retry::<T>(entity));
        commands.entity(entity).trigger(DeadlineReached::<T>::new);
    }
}

fn sync<T: Send + Sync + 'static>(
    clock: Res<Clock>,
    scheduler: Res<DeadlineScheduler>,
    deadlines: Query<(Entity, &Deadline<T>), Changed<Deadline<T>>>,
    mut debounces: Query<(Entity, &mut Debounce<T>), Changed<Debounce<T>>>,
    retries: Query<(Entity, &RetryBackoff<T>), Changed<RetryBackoff<T>>>,
) {
    let allow_worker = !clock.is_fake();
    let now = clock.now();
    for (entity, deadline) in &deadlines {
        scheduler.schedule(Key::deadline::<T>(entity), deadline.at, allow_worker);
    }
    for (entity, mut debounce) in &mut debounces {
        let at = match debounce.due_at {
            Some(at) => at,
            None => {
                let at = now + debounce.window;
                debounce.due_at = Some(at);
                at
            }
        };
        scheduler.schedule(Key::debounce::<T>(entity), at, allow_worker);
    }
    for (entity, retry) in &retries {
        match retry.next_retry {
            Some(at) => scheduler.schedule(Key::retry::<T>(entity), at, allow_worker),
            None => scheduler.cancel(Key::retry::<T>(entity)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::task::Wake;

    use crate::TimeWake;

    use super::*;

    struct Alpha;
    struct Beta;

    #[derive(Resource, Default)]
    struct Hits(Vec<Entity>);

    fn record_alpha(reached: On<DeadlineReached<Alpha>>, mut hits: ResMut<Hits>) {
        hits.0.push(reached.entity);
    }

    fn record_beta(reached: On<DeadlineReached<Beta>>, mut hits: ResMut<Hits>) {
        hits.0.push(reached.entity);
    }

    struct CountingWake {
        count: Arc<AtomicUsize>,
        notify: mpsc::SyncSender<()>,
    }

    impl Wake for CountingWake {
        fn wake(self: Arc<Self>) {
            self.count.fetch_add(1, Ordering::SeqCst);
            let _ = self.notify.try_send(());
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.count.fetch_add(1, Ordering::SeqCst);
            let _ = self.notify.try_send(());
        }
    }

    #[test]
    fn fake_clock_fires_deadlines_and_coalesces_earliest() {
        let clock = Clock::fake(1_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .init_resource::<Hits>()
            .add_plugins((
                DeadlinePlugin::<Alpha>::default(),
                DeadlinePlugin::<Beta>::default(),
            ))
            .add_observer(record_alpha)
            .add_observer(record_beta);

        let late = app
            .world_mut()
            .spawn(Deadline::<Beta>::after(&clock, Duration::from_secs(10)))
            .id();
        let early = app
            .world_mut()
            .spawn(Deadline::<Alpha>::after(&clock, Duration::from_secs(3)))
            .id();

        let scheduler = app.world().resource::<DeadlineScheduler>();
        assert_eq!(scheduler.pending_count(), 2);
        assert_eq!(
            scheduler.earliest(),
            Some(clock.now() + Duration::from_secs(3))
        );

        clock.advance(Duration::from_secs(2));
        app.update();
        assert!(app.world().resource::<Hits>().0.is_empty());

        clock.advance(Duration::from_secs(1));
        app.update();
        assert_eq!(app.world().resource::<Hits>().0, vec![early]);
        assert!(app.world().get::<Deadline<Alpha>>(early).is_none());
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            1
        );
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().earliest(),
            Some(clock.now() + Duration::from_secs(7))
        );

        clock.advance(Duration::from_secs(7));
        app.update();
        assert_eq!(app.world().resource::<Hits>().0, vec![early, late]);
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            0
        );
    }

    #[test]
    fn despawn_and_remove_cancel_pending_deadlines() {
        let clock = Clock::fake(1_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .init_resource::<Hits>()
            .add_plugins(DeadlinePlugin::<Alpha>::default())
            .add_observer(record_alpha);

        let despawned = app
            .world_mut()
            .spawn(Deadline::<Alpha>::after(&clock, Duration::from_secs(5)))
            .id();
        let removed = app
            .world_mut()
            .spawn(Deadline::<Alpha>::after(&clock, Duration::from_secs(5)))
            .id();

        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            2
        );
        app.world_mut().despawn(despawned);
        app.world_mut()
            .entity_mut(removed)
            .remove::<Deadline<Alpha>>();
        assert_eq!(
            app.world().resource::<DeadlineScheduler>().pending_count(),
            0
        );

        clock.advance(Duration::from_secs(10));
        app.update();
        assert!(app.world().resource::<Hits>().0.is_empty());
    }

    #[test]
    fn debounce_resets_on_reinsert_and_in_place_reset() {
        let clock = Clock::fake(1_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .init_resource::<Hits>()
            .add_plugins(DeadlinePlugin::<Alpha>::default())
            .add_observer(record_alpha);

        let entity = app
            .world_mut()
            .spawn(Debounce::<Alpha>::new(Duration::from_millis(500)))
            .id();
        app.update();

        clock.advance(Duration::from_millis(300));
        app.world_mut()
            .get_mut::<Debounce<Alpha>>(entity)
            .unwrap()
            .reset(&clock);
        app.update();

        clock.advance(Duration::from_millis(300));
        app.update();
        assert!(app.world().resource::<Hits>().0.is_empty());

        app.world_mut()
            .entity_mut(entity)
            .insert(Debounce::<Alpha>::new(Duration::from_millis(500)));
        clock.advance(Duration::from_millis(400));
        app.update();
        assert!(app.world().resource::<Hits>().0.is_empty());

        clock.advance(Duration::from_millis(100));
        app.update();
        assert_eq!(app.world().resource::<Hits>().0, vec![entity]);
        assert!(app.world().get::<Debounce<Alpha>>(entity).is_none());
    }

    #[test]
    fn retry_backoff_doubles_caps_and_respects_max_attempts() {
        let clock = Clock::fake(1_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .init_resource::<Hits>()
            .add_plugins(DeadlinePlugin::<Alpha>::default())
            .add_observer(record_alpha);

        let policy =
            BackoffPolicy::exponential(Duration::from_millis(100), Duration::from_millis(250))
                .with_max_attempts(3);
        let entity = app
            .world_mut()
            .spawn(RetryBackoff::<Alpha>::scheduled(policy, &clock))
            .id();

        assert_eq!(
            app.world()
                .get::<RetryBackoff<Alpha>>(entity)
                .unwrap()
                .attempts(),
            1
        );
        clock.advance(Duration::from_millis(99));
        app.update();
        assert!(app.world().resource::<Hits>().0.is_empty());

        clock.advance(Duration::from_millis(1));
        app.update();
        assert_eq!(app.world().resource::<Hits>().0.len(), 1);
        assert!(
            app.world()
                .get::<RetryBackoff<Alpha>>(entity)
                .unwrap()
                .next_retry()
                .is_none()
        );

        let second = app
            .world_mut()
            .get_mut::<RetryBackoff<Alpha>>(entity)
            .unwrap()
            .record_failure(&clock);
        assert_eq!(second, Some(clock.now() + Duration::from_millis(200)));
        clock.advance(Duration::from_millis(200));
        app.update();
        assert_eq!(app.world().resource::<Hits>().0.len(), 2);

        let third = app
            .world_mut()
            .get_mut::<RetryBackoff<Alpha>>(entity)
            .unwrap()
            .record_failure(&clock);
        assert_eq!(third, Some(clock.now() + Duration::from_millis(250)));
        clock.advance(Duration::from_millis(250));
        app.update();
        assert_eq!(app.world().resource::<Hits>().0.len(), 3);

        let mut retry = app
            .world_mut()
            .get_mut::<RetryBackoff<Alpha>>(entity)
            .unwrap();
        assert!(retry.exhausted());
        assert_eq!(retry.record_failure(&clock), None);
        retry.reset();
        assert_eq!(retry.attempts(), 0);
        assert!(!retry.exhausted());
    }

    #[test]
    fn real_scheduler_wakes_once_for_earliest_deadline_and_stops_cleanly_on_drop() {
        let mut app = App::new();
        let count = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::sync_channel(8);
        app.add_plugins(DeadlinePlugin::<Alpha>::default())
            .insert_resource(TimeWake(Waker::from(Arc::new(CountingWake {
                count: Arc::clone(&count),
                notify: tx,
            }))));

        let now = Instant::now();
        app.world_mut()
            .spawn(Deadline::<Alpha>::at(now + Duration::from_secs(30)));
        app.world_mut()
            .spawn(Deadline::<Alpha>::at(now + Duration::from_millis(20)));
        app.update();

        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(app);
    }
}
