use std::any::TypeId;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_tasks::{IoTaskPool, Task, TaskPool};

pub trait IoTaskList<E: Send + Sync + 'static = String> {
    fn register(app: &mut App);
}

pub struct IoTaskPlugin<T, E: Send + Sync + 'static = String>(PhantomData<fn() -> (T, E)>);

impl<T, E: Send + Sync + 'static> Default for IoTaskPlugin<T, E> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T, E> Plugin for IoTaskPlugin<T, E>
where
    T: IoTaskList<E> + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn build(&self, app: &mut App) {
        T::register(app);
    }

    fn is_unique(&self) -> bool {
        false
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IoTaskSet(TypeId);

impl IoTaskSet {
    pub fn all() -> Self {
        Self(TypeId::of::<Self>())
    }

    pub fn of<T: Component, E: Send + Sync + 'static>() -> Self {
        Self(TypeId::of::<(T, E)>())
    }
}

#[derive(Resource)]
struct RegisteredTask<T: Component, E: Send + Sync + 'static>(PhantomData<fn() -> (T, E)>);

impl<T: Component, E: Send + Sync + 'static> RegisteredTask<T, E> {
    fn ensure(app: &mut App) {
        if app.world().contains_resource::<Self>() {
            return;
        }
        app.insert_resource(Self(PhantomData)).add_systems(
            Update,
            poll::<T, E>
                .in_set(IoTaskSet::of::<T, E>())
                .in_set(IoTaskSet::all()),
        );
    }
}

impl<E: Send + Sync + 'static> IoTaskList<E> for () {
    fn register(_app: &mut App) {}
}

impl<E: Send + Sync + 'static, T0: Component> IoTaskList<E> for (T0,) {
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
    }
}

impl<E: Send + Sync + 'static, T0: Component, T1: Component> IoTaskList<E> for (T0, T1) {
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
    }
}

impl<E: Send + Sync + 'static, T0: Component, T1: Component, T2: Component> IoTaskList<E>
    for (T0, T1, T2)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
    }
}

impl<E: Send + Sync + 'static, T0: Component, T1: Component, T2: Component, T3: Component>
    IoTaskList<E> for (T0, T1, T2, T3)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4, T5)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4, T5, T6)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
    T7: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4, T5, T6, T7)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
        RegisteredTask::<T7, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
    T7: Component,
    T8: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4, T5, T6, T7, T8)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
        RegisteredTask::<T7, E>::ensure(app);
        RegisteredTask::<T8, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
    T7: Component,
    T8: Component,
    T9: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
        RegisteredTask::<T7, E>::ensure(app);
        RegisteredTask::<T8, E>::ensure(app);
        RegisteredTask::<T9, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
    T7: Component,
    T8: Component,
    T9: Component,
    T10: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
        RegisteredTask::<T7, E>::ensure(app);
        RegisteredTask::<T8, E>::ensure(app);
        RegisteredTask::<T9, E>::ensure(app);
        RegisteredTask::<T10, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
    T7: Component,
    T8: Component,
    T9: Component,
    T10: Component,
    T11: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
        RegisteredTask::<T7, E>::ensure(app);
        RegisteredTask::<T8, E>::ensure(app);
        RegisteredTask::<T9, E>::ensure(app);
        RegisteredTask::<T10, E>::ensure(app);
        RegisteredTask::<T11, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
    T7: Component,
    T8: Component,
    T9: Component,
    T10: Component,
    T11: Component,
    T12: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
        RegisteredTask::<T7, E>::ensure(app);
        RegisteredTask::<T8, E>::ensure(app);
        RegisteredTask::<T9, E>::ensure(app);
        RegisteredTask::<T10, E>::ensure(app);
        RegisteredTask::<T11, E>::ensure(app);
        RegisteredTask::<T12, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
    T7: Component,
    T8: Component,
    T9: Component,
    T10: Component,
    T11: Component,
    T12: Component,
    T13: Component,
> IoTaskList<E> for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13)
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
        RegisteredTask::<T7, E>::ensure(app);
        RegisteredTask::<T8, E>::ensure(app);
        RegisteredTask::<T9, E>::ensure(app);
        RegisteredTask::<T10, E>::ensure(app);
        RegisteredTask::<T11, E>::ensure(app);
        RegisteredTask::<T12, E>::ensure(app);
        RegisteredTask::<T13, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
    T7: Component,
    T8: Component,
    T9: Component,
    T10: Component,
    T11: Component,
    T12: Component,
    T13: Component,
    T14: Component,
> IoTaskList<E>
    for (
        T0,
        T1,
        T2,
        T3,
        T4,
        T5,
        T6,
        T7,
        T8,
        T9,
        T10,
        T11,
        T12,
        T13,
        T14,
    )
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
        RegisteredTask::<T7, E>::ensure(app);
        RegisteredTask::<T8, E>::ensure(app);
        RegisteredTask::<T9, E>::ensure(app);
        RegisteredTask::<T10, E>::ensure(app);
        RegisteredTask::<T11, E>::ensure(app);
        RegisteredTask::<T12, E>::ensure(app);
        RegisteredTask::<T13, E>::ensure(app);
        RegisteredTask::<T14, E>::ensure(app);
    }
}

impl<
    E: Send + Sync + 'static,
    T0: Component,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
    T6: Component,
    T7: Component,
    T8: Component,
    T9: Component,
    T10: Component,
    T11: Component,
    T12: Component,
    T13: Component,
    T14: Component,
    T15: Component,
> IoTaskList<E>
    for (
        T0,
        T1,
        T2,
        T3,
        T4,
        T5,
        T6,
        T7,
        T8,
        T9,
        T10,
        T11,
        T12,
        T13,
        T14,
        T15,
    )
{
    fn register(app: &mut App) {
        RegisteredTask::<T0, E>::ensure(app);
        RegisteredTask::<T1, E>::ensure(app);
        RegisteredTask::<T2, E>::ensure(app);
        RegisteredTask::<T3, E>::ensure(app);
        RegisteredTask::<T4, E>::ensure(app);
        RegisteredTask::<T5, E>::ensure(app);
        RegisteredTask::<T6, E>::ensure(app);
        RegisteredTask::<T7, E>::ensure(app);
        RegisteredTask::<T8, E>::ensure(app);
        RegisteredTask::<T9, E>::ensure(app);
        RegisteredTask::<T10, E>::ensure(app);
        RegisteredTask::<T11, E>::ensure(app);
        RegisteredTask::<T12, E>::ensure(app);
        RegisteredTask::<T13, E>::ensure(app);
        RegisteredTask::<T14, E>::ensure(app);
        RegisteredTask::<T15, E>::ensure(app);
    }
}

#[derive(Resource, Clone)]
pub struct TaskWake(pub Waker);

impl TaskWake {
    pub fn wake(&self) {
        self.0.wake_by_ref();
    }
}

#[derive(Component)]
#[component(on_insert = prepare::<T, E>)]
pub struct IoTask<T: Component, E: Send + Sync + 'static = String> {
    task: Option<Task<Result<T, E>>>,
}

impl<T: Component, E: Send + Sync + 'static> IoTask<T, E> {
    pub fn new(task: Task<Result<T, E>>) -> Self {
        Self { task: Some(task) }
    }

    pub fn spawn<F>(future: F) -> Self
    where
        F: Future<Output = Result<T, E>> + Send + 'static,
    {
        Self::new(IoTaskPool::get_or_init(TaskPool::new).spawn(future))
    }
}

#[derive(Component)]
pub struct TaskError<T: Component, E: Send + Sync + 'static = String> {
    error: Option<E>,
    marker: PhantomData<fn() -> T>,
}

impl<T: Component, E: Send + Sync + 'static> TaskError<T, E> {
    pub fn failed(error: E) -> Self {
        Self {
            error: Some(error),
            marker: PhantomData,
        }
    }

    pub fn panicked() -> Self {
        Self {
            error: None,
            marker: PhantomData,
        }
    }

    pub fn error(&self) -> Option<&E> {
        self.error.as_ref()
    }

    pub fn into_error(self) -> Option<E> {
        self.error
    }

    pub fn is_panic(&self) -> bool {
        self.error.is_none()
    }
}

impl<T: Component, E: Clone + Send + Sync + 'static> Clone for TaskError<T, E> {
    fn clone(&self) -> Self {
        Self {
            error: self.error.clone(),
            marker: PhantomData,
        }
    }
}

impl<T: Component, E: fmt::Debug + Send + Sync + 'static> fmt::Debug for TaskError<T, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskError")
            .field("error", &self.error)
            .finish()
    }
}

impl<T: Component, E: PartialEq + Send + Sync + 'static> PartialEq for TaskError<T, E> {
    fn eq(&self, other: &Self) -> bool {
        self.error == other.error
    }
}

impl<T: Component, E: Eq + Send + Sync + 'static> Eq for TaskError<T, E> {}

impl<T: Component, E: fmt::Display + Send + Sync + 'static> fmt::Display for TaskError<T, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.error {
            Some(error) => error.fmt(formatter),
            None => formatter.write_str("asynchronous task panicked"),
        }
    }
}

impl<T: Component, E: std::error::Error + Send + Sync + 'static> std::error::Error
    for TaskError<T, E>
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error
            .as_ref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

struct Finish<T: Component, E: Send + Sync + 'static> {
    entity: Entity,
    outcome: Result<T, TaskError<T, E>>,
}

impl<T: Component, E: Send + Sync + 'static> Command for Finish<T, E> {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Ok(mut entity) = world.get_entity_mut(self.entity) else {
            return;
        };
        if entity
            .get::<IoTask<T, E>>()
            .is_none_or(|pending| pending.task.is_some())
        {
            return;
        }
        entity.remove::<IoTask<T, E>>();
        match self.outcome {
            Ok(value) => {
                entity.insert(value);
            }
            Err(error) => {
                entity.insert(error);
            }
        }
    }
}

fn prepare<T: Component, E: Send + Sync + 'static>(mut world: DeferredWorld, context: HookContext) {
    let entity = context.entity;
    if world.get::<TaskError<T, E>>(entity).is_some() {
        world.commands().entity(entity).remove::<TaskError<T, E>>();
    }
    if let Some(wake) = world.get_resource::<TaskWake>() {
        wake.wake();
    }
}

fn poll<T: Component, E: Send + Sync + 'static>(
    mut tasks: Query<(Entity, &mut IoTask<T, E>)>,
    wake: Option<Res<TaskWake>>,
    mut commands: Commands,
) {
    let waker = wake.as_deref().map(|wake| &wake.0).unwrap_or(Waker::noop());
    let mut context = Context::from_waker(waker);
    for (entity, mut pending) in &mut tasks {
        let Some(task) = pending.task.as_mut() else {
            continue;
        };
        let outcome = match catch_unwind(AssertUnwindSafe(|| Pin::new(task).poll(&mut context))) {
            Ok(Poll::Ready(Ok(value))) => Ok(value),
            Ok(Poll::Ready(Err(error))) => Err(TaskError::failed(error)),
            Ok(Poll::Pending) => continue,
            Err(_) => Err(TaskError::panicked()),
        };
        pending.task.take();
        waker.wake_by_ref();
        commands.queue(Finish { entity, outcome });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::task::Wake;
    use std::time::{Duration, Instant};

    use bevy_tasks::TaskPoolBuilder;
    use bevy_tasks::futures_lite::future;

    use super::*;

    #[derive(Component, Debug, PartialEq, Eq)]
    struct Loaded(u32);

    #[derive(Component, Debug, PartialEq, Eq)]
    struct Secondary(u32);

    struct Notify(mpsc::SyncSender<()>);

    impl Wake for Notify {
        fn wake(self: Arc<Self>) {
            let _ = self.0.try_send(());
        }

        fn wake_by_ref(self: &Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    struct DropNotice(mpsc::SyncSender<()>);

    impl Drop for DropNotice {
        fn drop(&mut self) {
            let _ = self.0.try_send(());
        }
    }

    struct Fixture {
        app: App,
        pool: TaskPool,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((
                IoTaskPlugin::<(Loaded, Secondary)>::default(),
                IoTaskPlugin::<(Loaded,)>::default(),
                IoTaskPlugin::<(Loaded,)>::default(),
            ));
            Self {
                app,
                pool: TaskPoolBuilder::new().num_threads(1).build(),
            }
        }

        fn wait_until(&mut self, check: impl Fn(&World) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                self.app.update();
                if check(self.app.world()) {
                    return;
                }
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
    }

    #[test]
    fn completing_an_io_task_inserts_its_component_and_removes_the_task() {
        let mut fixture = Fixture::new();
        let entity = fixture
            .app
            .world_mut()
            .spawn(IoTask::new(
                fixture.pool.spawn(async { Ok::<_, String>(Loaded(7)) }),
            ))
            .id();
        fixture.wait_until(|world| world.get::<Loaded>(entity).is_some());
        assert_eq!(fixture.app.world().get::<Loaded>(entity), Some(&Loaded(7)));
        assert!(fixture.app.world().get::<IoTask<Loaded>>(entity).is_none());
        assert!(
            fixture
                .app
                .world()
                .get::<TaskError<Loaded>>(entity)
                .is_none()
        );
    }

    #[test]
    fn failing_or_panicking_inserts_task_error_and_retry_clears_stale_error() {
        let mut fixture = Fixture::new();
        let entity = fixture
            .app
            .world_mut()
            .spawn(IoTask::new(
                fixture
                    .pool
                    .spawn(async { Err::<Loaded, String>("boom".into()) }),
            ))
            .id();
        fixture.wait_until(|world| world.get::<TaskError<Loaded>>(entity).is_some());
        let error = fixture
            .app
            .world()
            .get::<TaskError<Loaded>>(entity)
            .unwrap();
        assert!(!error.is_panic());
        assert_eq!(error.error().map(String::as_str), Some("boom"));
        assert_eq!(error.to_string(), "boom");

        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(IoTask::new(fixture.pool.spawn::<Result<Loaded, String>>(
                async {
                    panic!("worker panic");
                },
            )));
        assert!(
            fixture
                .app
                .world()
                .get::<TaskError<Loaded>>(entity)
                .is_none()
        );
        fixture.wait_until(|world| world.get::<TaskError<Loaded>>(entity).is_some());
        let panic_error = fixture
            .app
            .world()
            .get::<TaskError<Loaded>>(entity)
            .unwrap();
        assert!(panic_error.is_panic());
        assert_eq!(panic_error.error(), None);
        assert_eq!(panic_error.to_string(), "asynchronous task panicked");

        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(IoTask::new(
                fixture.pool.spawn(async { Ok::<_, String>(Loaded(9)) }),
            ));
        assert!(
            fixture
                .app
                .world()
                .get::<TaskError<Loaded>>(entity)
                .is_none()
        );
        fixture.wait_until(|world| world.get::<Loaded>(entity).is_some());
        assert_eq!(fixture.app.world().get::<Loaded>(entity), Some(&Loaded(9)));
        assert!(
            fixture
                .app
                .world()
                .get::<TaskError<Loaded>>(entity)
                .is_none()
        );
    }

    #[test]
    fn task_wake_fires_on_insert_and_on_background_completion() {
        let mut fixture = Fixture::new();
        let (wake_tx, wake_rx) = mpsc::sync_channel(4);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        fixture
            .app
            .insert_resource(TaskWake(Waker::from(Arc::new(Notify(wake_tx)))));
        let entity = fixture
            .app
            .world_mut()
            .spawn(IoTask::new(fixture.pool.spawn(async move {
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                Ok::<_, String>(Loaded(4))
            })))
            .id();
        wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        fixture.app.update();
        assert!(wake_rx.try_recv().is_err());
        assert!(fixture.app.world().get::<Loaded>(entity).is_none());
        release_tx.send(()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        fixture.app.update();
        assert_eq!(fixture.app.world().get::<Loaded>(entity), Some(&Loaded(4)));
        wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn replacing_or_despawning_an_entity_cancels_its_in_flight_io_task() {
        let mut fixture = Fixture::new();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (dropped_tx, dropped_rx) = mpsc::sync_channel(1);
        let first = fixture.pool.spawn(async move {
            let _notice = DropNotice(dropped_tx);
            started_tx.send(()).unwrap();
            future::pending::<Result<Loaded, String>>().await
        });
        let entity = fixture.app.world_mut().spawn(IoTask::new(first)).id();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();

        let (second_started_tx, second_started_rx) = mpsc::sync_channel(1);
        let (second_dropped_tx, second_dropped_rx) = mpsc::sync_channel(1);
        let second = fixture.pool.spawn(async move {
            let _notice = DropNotice(second_dropped_tx);
            second_started_tx.send(()).unwrap();
            future::pending::<Result<Loaded, String>>().await
        });
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(IoTask::new(second));
        dropped_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        second_started_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap();

        fixture.app.world_mut().entity_mut(entity).despawn();
        second_dropped_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
    }

    #[test]
    fn tuple_registration_polls_every_member_and_ignores_duplicate_registrations() {
        let mut fixture = Fixture::new();
        let first = fixture
            .app
            .world_mut()
            .spawn(IoTask::new(
                fixture.pool.spawn(async { Ok::<_, String>(Loaded(11)) }),
            ))
            .id();
        let second = fixture
            .app
            .world_mut()
            .spawn(IoTask::new(
                fixture.pool.spawn(async { Ok::<_, String>(Secondary(22)) }),
            ))
            .id();
        fixture.wait_until(|world| {
            world.get::<Loaded>(first).is_some() && world.get::<Secondary>(second).is_some()
        });
        assert_eq!(fixture.app.world().get::<Loaded>(first), Some(&Loaded(11)));
        assert_eq!(
            fixture.app.world().get::<Secondary>(second),
            Some(&Secondary(22))
        );
    }
}
