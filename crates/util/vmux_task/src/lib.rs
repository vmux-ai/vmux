use std::any::TypeId;
use std::future::Future;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Context, Poll, Waker};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_tasks::Task;

const ACTIVE: u8 = 0;
const APPLIED: u8 = 1;
const CANCELED: u8 = 2;

pub struct TaskPlugin<T: Send + Sync + 'static>(PhantomData<fn() -> T>);

impl<T: Send + Sync + 'static> Default for TaskPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync + 'static> Plugin for TaskPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (collect::<T>, discard::<T>)
                .chain()
                .in_set(TaskSet::of::<T>()),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskSet(TypeId);

impl TaskSet {
    pub fn of<T: Send + Sync + 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

#[derive(Component, Clone)]
pub struct TaskToken {
    id: uuid::Uuid,
    state: Arc<AtomicU8>,
}

impl Default for TaskToken {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::nil(),
            state: Arc::new(AtomicU8::new(CANCELED)),
        }
    }
}

impl TaskToken {
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            state: Arc::new(AtomicU8::new(ACTIVE)),
        }
    }

    pub fn is_active(&self) -> bool {
        self.state.load(Ordering::Acquire) == ACTIVE
    }

    pub fn is_canceled(&self) -> bool {
        self.state.load(Ordering::Acquire) == CANCELED
    }

    pub fn cancel(&self) {
        let _ = self
            .state
            .compare_exchange(ACTIVE, CANCELED, Ordering::AcqRel, Ordering::Acquire);
    }

    fn same(&self, other: &Self) -> bool {
        self.id == other.id
    }

    fn claim(&self, current: &Self) -> bool {
        self.same(current)
            && self
                .state
                .compare_exchange(ACTIVE, APPLIED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }
}

#[derive(Component)]
#[require(TaskToken, TaskWaker)]
#[component(on_insert = bind::<T>)]
pub struct AsyncTask<T: Send + Sync + 'static> {
    task: Option<Task<T>>,
    token: TaskToken,
}

#[derive(Component, Default)]
pub struct TaskWaker(Option<Waker>);

impl TaskWaker {
    pub fn new(waker: Waker) -> Self {
        Self(Some(waker))
    }

    pub fn is_bound(&self) -> bool {
        self.0.is_some()
    }

    pub fn bind(&mut self, waker: Waker) {
        self.0 = Some(waker);
    }

    fn current(&self) -> &Waker {
        self.0.as_ref().unwrap_or(Waker::noop())
    }
}

impl<T: Send + Sync + 'static> AsyncTask<T> {
    pub fn new(task: Task<T>) -> Self {
        Self {
            task: Some(task),
            token: TaskToken::new(),
        }
    }
}

impl<T: Send + Sync + 'static> Drop for AsyncTask<T> {
    fn drop(&mut self) {
        if self.task.is_some() {
            self.token.cancel();
        }
    }
}

fn bind<T: Send + Sync + 'static>(mut world: DeferredWorld, context: HookContext) {
    let token = world
        .get::<AsyncTask<T>>(context.entity)
        .unwrap()
        .token
        .clone();
    let mut current = world.get_mut::<TaskToken>(context.entity).unwrap();
    current.cancel();
    *current = token;
}

#[derive(Component)]
pub struct TaskResult<T: Send + Sync + 'static> {
    token: TaskToken,
    value: Option<Result<T, TaskFailure>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskFailure {
    Panicked,
}

impl std::fmt::Display for TaskFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Panicked => formatter.write_str("asynchronous task panicked"),
        }
    }
}

impl<T: Send + Sync + 'static> TaskResult<T> {
    pub fn take(&mut self) -> Option<Result<T, TaskFailure>> {
        self.value.take()
    }

    pub fn guard<C: Command<Out = ()>>(&self, entity: Entity, command: C) -> TaskApply<T, C> {
        TaskApply {
            entity,
            token: self.token.clone(),
            command,
            marker: PhantomData,
        }
    }
}

pub struct TaskApply<T: Send + Sync + 'static, C: Command<Out = ()>> {
    entity: Entity,
    token: TaskToken,
    command: C,
    marker: PhantomData<fn() -> T>,
}

impl<T: Send + Sync + 'static, C: Command<Out = ()>> Command for TaskApply<T, C> {
    type Out = ();

    fn apply(self, world: &mut World) -> Self::Out {
        let Some(current) = world.get::<TaskToken>(self.entity) else {
            return;
        };
        if !self.token.claim(current) {
            return;
        }
        if world
            .get::<TaskResult<T>>(self.entity)
            .is_some_and(|result| result.token.same(&self.token))
        {
            world.entity_mut(self.entity).remove::<TaskResult<T>>();
        }
        self.command.apply(world);
    }
}

struct Completion<T: Send + Sync + 'static> {
    entity: Entity,
    token: TaskToken,
    value: Result<T, TaskFailure>,
}

impl<T: Send + Sync + 'static> Command for Completion<T> {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Some(task) = world.get::<AsyncTask<T>>(self.entity) else {
            return;
        };
        if !task.token.same(&self.token) {
            return;
        }
        let current = world.get::<TaskToken>(self.entity).unwrap();
        if !self.token.same(current) || !self.token.is_active() {
            world.entity_mut(self.entity).remove::<AsyncTask<T>>();
            return;
        }
        world
            .entity_mut(self.entity)
            .remove::<AsyncTask<T>>()
            .insert(TaskResult {
                token: self.token,
                value: Some(self.value),
            });
    }
}

struct Discard<T: Send + Sync + 'static> {
    entity: Entity,
    token: TaskToken,
    marker: PhantomData<fn() -> T>,
}

impl<T: Send + Sync + 'static> Command for Discard<T> {
    type Out = ();

    fn apply(self, world: &mut World) {
        if world
            .get::<TaskResult<T>>(self.entity)
            .is_some_and(|result| result.token.same(&self.token))
        {
            world.entity_mut(self.entity).remove::<TaskResult<T>>();
        }
        if world
            .get::<AsyncTask<T>>(self.entity)
            .is_some_and(|task| task.token.same(&self.token))
        {
            world.entity_mut(self.entity).remove::<AsyncTask<T>>();
        }
    }
}

fn collect<T: Send + Sync + 'static>(
    mut tasks: Query<(Entity, &mut AsyncTask<T>)>,
    tokens: Query<&TaskToken>,
    wakers: Query<&TaskWaker>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let current = tokens.get(entity).unwrap();
        if !pending.token.same(current) || !pending.token.is_active() {
            commands.queue(Discard::<T> {
                entity,
                token: pending.token.clone(),
                marker: PhantomData,
            });
            continue;
        }
        let Some(task) = pending.task.as_mut() else {
            continue;
        };
        let mut context = Context::from_waker(wakers.get(entity).unwrap().current());
        let value = match catch_unwind(AssertUnwindSafe(|| Pin::new(task).poll(&mut context))) {
            Ok(Poll::Ready(value)) => Ok(value),
            Ok(Poll::Pending) => continue,
            Err(_) => Err(TaskFailure::Panicked),
        };
        pending.task.take();
        commands.queue(Completion {
            entity,
            token: pending.token.clone(),
            value,
        });
    }
}

fn discard<T: Send + Sync + 'static>(
    results: Query<(Entity, &TaskResult<T>)>,
    tokens: Query<&TaskToken>,
    mut commands: Commands,
) {
    for (entity, result) in &results {
        if !tokens
            .get(entity)
            .is_ok_and(|current| result.token.same(current))
            || !result.token.is_active()
        {
            commands.queue(Discard::<T> {
                entity,
                token: result.token.clone(),
                marker: PhantomData,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use bevy_tasks::{TaskPool, TaskPoolBuilder};

    use super::*;

    #[derive(Resource, Default)]
    struct Applied(Vec<u32>);

    struct Append(u32);

    impl Command for Append {
        type Out = ();

        fn apply(self, world: &mut World) {
            world.resource_mut::<Applied>().0.push(self.0);
        }
    }

    struct Fixture {
        app: App,
        pool: TaskPool,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(TaskPlugin::<u32>::default())
                .init_resource::<Applied>();
            Self {
                app,
                pool: TaskPoolBuilder::new().num_threads(1).build(),
            }
        }

        fn start(&mut self, value: u32) -> Entity {
            self.app
                .world_mut()
                .spawn(AsyncTask::new(self.pool.spawn(async move { value })))
                .id()
        }

        fn ready(&mut self, entity: Entity) {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                self.app.update();
                if self.app.world().get::<TaskResult<u32>>(entity).is_some() {
                    return;
                }
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
        }

        fn command(&mut self, entity: Entity) -> TaskApply<u32, Append> {
            let mut result = self
                .app
                .world_mut()
                .get_mut::<TaskResult<u32>>(entity)
                .unwrap();
            let value = result.take().unwrap().unwrap();
            result.guard(entity, Append(value))
        }
    }

    #[test]
    fn replacing_a_task_rejects_a_previously_collected_result_at_the_commit_boundary() {
        let mut fixture = Fixture::new();
        let entity = fixture.start(1);
        fixture.ready(entity);
        let previous = fixture.command(entity);
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(AsyncTask::new(fixture.pool.spawn(async { 2u32 })));
        fixture.app.world_mut().commands().queue(previous);
        fixture.app.world_mut().flush();
        assert!(fixture.app.world().resource::<Applied>().0.is_empty());
        fixture.ready(entity);
        let latest = fixture.command(entity);
        fixture.app.world_mut().commands().queue(latest);
        fixture.app.world_mut().flush();
        assert_eq!(fixture.app.world().resource::<Applied>().0, [2]);
    }

    #[test]
    fn cancellation_after_collection_prevents_result_application() {
        let mut fixture = Fixture::new();
        let entity = fixture.start(1);
        fixture.ready(entity);
        let result = fixture.command(entity);
        fixture
            .app
            .world()
            .get::<TaskToken>(entity)
            .unwrap()
            .cancel();
        fixture.app.world_mut().commands().queue(result);
        fixture.app.world_mut().flush();
        fixture.app.update();
        assert!(fixture.app.world().resource::<Applied>().0.is_empty());
        assert!(fixture.app.world().get::<TaskResult<u32>>(entity).is_none());
    }

    #[test]
    fn a_result_can_be_claimed_once_even_if_two_commands_share_its_token() {
        let mut fixture = Fixture::new();
        let entity = fixture.start(1);
        fixture.ready(entity);
        let mut result = fixture
            .app
            .world_mut()
            .get_mut::<TaskResult<u32>>(entity)
            .unwrap();
        let value = result.take().unwrap().unwrap();
        let first = result.guard(entity, Append(value));
        let second = result.guard(entity, Append(2));
        fixture.app.world_mut().commands().queue(first);
        fixture.app.world_mut().commands().queue(second);
        fixture.app.world_mut().flush();
        assert_eq!(fixture.app.world().resource::<Applied>().0, [1]);
    }

    #[test]
    fn removing_the_owner_discards_a_collected_result() {
        let mut fixture = Fixture::new();
        let entity = fixture.start(1);
        fixture.ready(entity);
        let result = fixture.command(entity);
        fixture.app.world_mut().entity_mut(entity).despawn();
        fixture.app.world_mut().commands().queue(result);
        fixture.app.world_mut().flush();
        assert!(fixture.app.world().resource::<Applied>().0.is_empty());
    }

    #[test]
    fn a_panicking_task_becomes_a_failure_and_does_not_stop_following_work() {
        let mut fixture = Fixture::new();
        let entity = fixture
            .app
            .world_mut()
            .spawn(AsyncTask::new(fixture.pool.spawn::<u32>(async {
                panic!("task failure");
            })))
            .id();
        fixture.ready(entity);
        assert_eq!(
            fixture
                .app
                .world_mut()
                .get_mut::<TaskResult<u32>>(entity)
                .unwrap()
                .take(),
            Some(Err(TaskFailure::Panicked))
        );
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(AsyncTask::new(fixture.pool.spawn(async { 3u32 })));
        fixture.ready(entity);
        let result = fixture.command(entity);
        fixture.app.world_mut().commands().queue(result);
        fixture.app.world_mut().flush();
        assert_eq!(fixture.app.world().resource::<Applied>().0, [3]);
    }

    struct Notify(std::sync::mpsc::SyncSender<()>);

    impl std::task::Wake for Notify {
        fn wake(self: Arc<Self>) {
            let _ = self.0.try_send(());
        }

        fn wake_by_ref(self: &Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    #[test]
    fn completion_wakes_the_consumer_only_after_the_result_is_ready() {
        let mut fixture = Fixture::new();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let (wake_tx, wake_rx) = std::sync::mpsc::sync_channel(1);
        let entity = fixture
            .app
            .world_mut()
            .spawn((
                AsyncTask::new(fixture.pool.spawn(async move {
                    release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    4u32
                })),
                TaskWaker::new(Waker::from(Arc::new(Notify(wake_tx)))),
            ))
            .id();
        fixture.app.update();
        assert!(fixture.app.world().get::<TaskResult<u32>>(entity).is_none());
        release_tx.send(()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        fixture.app.update();
        let result = fixture.command(entity);
        fixture.app.world_mut().commands().queue(result);
        fixture.app.world_mut().flush();
        assert_eq!(fixture.app.world().resource::<Applied>().0, [4]);
    }

    struct DropNotice(std::sync::mpsc::SyncSender<()>);

    impl Drop for DropNotice {
        fn drop(&mut self) {
            let _ = self.0.try_send(());
        }
    }

    #[test]
    fn removing_a_pending_task_owner_cancels_and_releases_its_future() {
        let mut fixture = Fixture::new();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let (released_tx, released_rx) = std::sync::mpsc::sync_channel(1);
        let task = fixture.pool.spawn(async move {
            let _notice = DropNotice(released_tx);
            started_tx.send(()).unwrap();
            bevy_tasks::futures_lite::future::pending::<u32>().await
        });
        let entity = fixture.app.world_mut().spawn(AsyncTask::new(task)).id();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let token = fixture
            .app
            .world()
            .get::<TaskToken>(entity)
            .unwrap()
            .clone();
        fixture.app.world_mut().entity_mut(entity).despawn();
        released_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(token.is_canceled());
        assert!(fixture.app.world().resource::<Applied>().0.is_empty());
    }
}
