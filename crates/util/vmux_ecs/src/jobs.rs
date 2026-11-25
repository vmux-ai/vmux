use std::any::Any;
use std::marker::PhantomData;
use std::task::Waker;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;

use crate::blocking::{BlockingRuntime, BlockingTask, Lane, SpawnError};
use crate::channel::ChannelWake;
use crate::{Active, Operation, OperationPlugin, OperationState, OperationTarget};

pub struct JobsPlugin;

impl Plugin for JobsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.init_resource::<BlockingRuntime>()
            .configure_sets(
                Update,
                (JobSet::Submit, JobSet::Collect, JobSet::Apply).chain(),
            )
            .configure_sets(PostUpdate, JobSet::Cleanup);
    }
}

pub struct JobPlugin<J: Job>(PhantomData<fn() -> J>);

impl<J: Job> Default for JobPlugin<J> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<J: Job> Plugin for JobPlugin<J> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<JobsPlugin>() {
            app.add_plugins(JobsPlugin);
        }
        app.add_message::<JobReady<J>>()
            .add_observer(retire::<J>)
            .add_systems(
                Update,
                (cancel::<J>, accept::<J>, submit::<J>)
                    .chain()
                    .in_set(JobSet::Submit),
            )
            .add_systems(Update, collect::<J>.in_set(JobSet::Collect))
            .add_systems(
                PostUpdate,
                (cancel::<J>, cleanup::<J>).chain().in_set(JobSet::Cleanup),
            )
            .add_systems(PostUpdate, wake::<J>.after(JobSet::Cleanup));
    }
}

pub trait Job: Clone + Send + Sync + 'static {
    type Output: Send + Sync + 'static;

    fn run(self) -> Self::Output;

    fn lane(&self) -> Lane {
        Lane::Short
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobSet {
    Submit,
    Collect,
    Apply,
    Cleanup,
}

#[derive(Resource, Clone)]
#[component(on_insert = sync_channel_wake)]
pub struct JobWake(pub Waker);

fn sync_channel_wake(mut world: DeferredWorld, context: HookContext) {
    let waker = world.get::<JobWake>(context.entity).unwrap().0.clone();
    world.commands().insert_resource(ChannelWake(waker));
}

#[derive(Component)]
#[component(immutable)]
pub struct JobRequest<J: Job>(pub J);

#[derive(Component)]
pub struct JobResult<J: Job>(pub Result<J::Output, String>);

#[derive(Message)]
pub struct JobReady<J: Job> {
    pub entity: Entity,
    marker: PhantomData<fn() -> J>,
}

impl<J: Job> JobReady<J> {
    fn of(entity: Entity) -> Self {
        Self {
            entity,
            marker: PhantomData,
        }
    }
}

#[derive(Component)]
struct JobRunning<J: Job> {
    task: BlockingTask<J::Output>,
    canceled: bool,
}

#[derive(Component)]
#[component(immutable)]
struct CapturedJob<J: Job> {
    input: J,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    owner: JobOwner<J>,
}

#[derive(Component)]
pub struct JobCapture;

#[derive(Component)]
#[component(immutable)]
struct JobSubmitted<J: Job> {
    task: Entity,
    marker: PhantomData<fn() -> J>,
}

#[derive(Component)]
#[component(immutable)]
pub struct JobOwner<J: Job> {
    owner: Option<Entity>,
    marker: PhantomData<fn() -> J>,
}

impl<J: Job> JobOwner<J> {
    pub fn accepts(&self, parent: Option<&ChildOf>) -> bool {
        parent.map(ChildOf::parent) == self.owner
    }

    fn of(parent: Option<&ChildOf>) -> Self {
        Self {
            owner: parent.map(ChildOf::parent),
            marker: PhantomData,
        }
    }
}

type PendingJobs<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobRequest<J>,
        Option<&'static ChildOf>,
        &'static OperationState,
        Option<&'static JobOwner<J>>,
    ),
    (
        With<Operation>,
        Without<JobSubmitted<J>>,
        Without<JobResult<J>>,
    ),
>;

type RunningJobs<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut JobRunning<J>,
        &'static CapturedJob<J>,
        Option<&'static OperationTarget>,
    ),
    With<Operation>,
>;

type CapturedJobs<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static CapturedJob<J>,
        Option<&'static OperationTarget>,
    ),
    (With<Operation>, Without<JobRunning<J>>),
>;

type RequestingJobs<'w, 's, J> = Query<
    'w,
    's,
    (
        &'static OperationState,
        &'static JobSubmitted<J>,
        Option<&'static ChildOf>,
    ),
    With<Operation>,
>;

type OwnedJobs<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobOwner<J>,
        Option<&'static ChildOf>,
        &'static OperationState,
        Option<&'static JobSubmitted<J>>,
    ),
    With<Operation>,
>;

type CompletedJobs<'w, 's, J> = Query<
    'w,
    's,
    (Entity, &'static OperationState),
    (
        With<Operation>,
        Or<(
            With<JobRequest<J>>,
            With<JobSubmitted<J>>,
            With<JobResult<J>>,
        )>,
    ),
>;

fn accept<J: Job>(requests: PendingJobs<J>, mut commands: Commands) {
    for (entity, request, parent, state, owner) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if let Some(owner) = owner {
            if !owner.accepts(parent) {
                continue;
            }
        } else {
            commands.entity(entity).insert(JobOwner::<J>::of(parent));
        }
        let native = commands
            .spawn((
                Operation::bundle("native-blocking-job").expect("fixed native job operation kind"),
                Name::new("Native blocking job"),
                Active,
                OperationTarget::new(entity),
                CapturedJob {
                    input: request.0.clone(),
                    owner: JobOwner::of(parent),
                },
                JobCapture,
            ))
            .id();
        commands.entity(entity).insert(JobSubmitted::<J> {
            task: native,
            marker: PhantomData,
        });
    }
}

fn submit<J: Job>(
    captured: CapturedJobs<J>,
    requests: RequestingJobs<J>,
    runtime: Res<BlockingRuntime>,
    wake: Option<Res<JobWake>>,
    mut ready: MessageWriter<JobReady<J>>,
    mut commands: Commands,
) {
    for (native, captured, target) in &captured {
        let Some(target) = target else {
            commands.entity(native).try_despawn();
            continue;
        };
        let owner = target.entity();
        let accepted = match requests.get(owner) {
            Ok((state, ticket, parent)) => {
                matches!(state, OperationState::Pending | OperationState::Running)
                    && ticket.task == native
                    && captured.owner.accepts(parent)
            }
            Err(_) => false,
        };
        if !accepted {
            commands.entity(native).try_despawn();
            continue;
        }
        let job = captured.input.clone();
        let lane = job.lane();
        let wake = wake.as_deref().map(|wake| wake.0.clone());
        match runtime.try_spawn(lane, wake, move || job.run()) {
            Ok(task) => {
                commands.entity(native).insert((
                    OperationState::Running,
                    JobRunning::<J> {
                        task,
                        canceled: false,
                    },
                ));
                commands
                    .entity(owner)
                    .remove::<JobRequest<J>>()
                    .insert(OperationState::Running);
            }
            Err(SpawnError::Full) => {}
            Err(error) => {
                commands
                    .entity(owner)
                    .remove::<JobSubmitted<J>>()
                    .insert(JobResult::<J>(Err(error.to_string())));
                commands.entity(native).try_despawn();
                ready.write(JobReady::of(owner));
            }
        }
    }
}

fn collect<J: Job>(
    mut running: RunningJobs<J>,
    requests: RequestingJobs<J>,
    mut ready: MessageWriter<JobReady<J>>,
    mut commands: Commands,
) {
    for (native, mut running, captured, target) in &mut running {
        let accepted = match target {
            Some(target) => match requests.get(target.entity()) {
                Ok((state, ticket, parent)) => {
                    matches!(state, OperationState::Pending | OperationState::Running)
                        && ticket.task == native
                        && captured.owner.accepts(parent)
                }
                Err(_) => false,
            },
            None => false,
        };
        if !accepted || running.canceled {
            if !running.canceled {
                running.task.cancel();
                running.canceled = true;
            }
            if running.task.is_finished() {
                commands.entity(native).try_despawn();
            }
            continue;
        }
        if !running.task.is_finished() {
            continue;
        }
        let Some(result) = running.task.poll() else {
            continue;
        };
        let owner = target.unwrap().entity();
        let result = result.map_err(|panic| WorkerPanic(panic).message());
        commands
            .entity(owner)
            .remove::<JobSubmitted<J>>()
            .insert(JobResult::<J>(result));
        commands.entity(native).try_despawn();
        ready.write(JobReady::of(owner));
    }
}

fn cancel<J: Job>(
    jobs: OwnedJobs<J>,
    mut running: Query<&mut JobRunning<J>>,
    mut commands: Commands,
) {
    for (entity, owner, parent, state, submitted) in &jobs {
        let changed_owner = !owner.accepts(parent);
        if !changed_owner && matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if let Some(submitted) = submitted {
            match running.get_mut(submitted.task) {
                Ok(mut running) if !running.canceled => {
                    running.task.cancel();
                    running.canceled = true;
                }
                Ok(_) => {}
                Err(_) => {
                    commands.entity(submitted.task).try_despawn();
                }
            }
        }
        commands
            .entity(entity)
            .remove::<(JobSubmitted<J>, JobRequest<J>, JobResult<J>, JobOwner<J>)>();
        if changed_owner && matches!(state, OperationState::Pending | OperationState::Running) {
            commands.entity(entity).insert(OperationState::Canceled);
        }
    }
}

fn retire<J: Job>(
    removed: On<Remove, OperationTarget>,
    mut running: Query<&mut JobRunning<J>>,
    captured: Query<(), With<CapturedJob<J>>>,
    mut commands: Commands,
) {
    let native = removed.event_target();
    match running.get_mut(native) {
        Ok(mut running) if !running.canceled => {
            running.task.cancel();
            running.canceled = true;
        }
        Ok(_) => {}
        Err(_) if captured.contains(native) => {
            commands.entity(native).try_despawn();
        }
        Err(_) => {}
    }
}

fn cleanup<J: Job>(jobs: CompletedJobs<J>, mut commands: Commands) {
    for (entity, state) in &jobs {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(JobRequest<J>, JobSubmitted<J>, JobResult<J>, JobOwner<J>)>();
    }
}

fn wake<J: Job>(
    ready: Query<Entity, Added<JobResult<J>>>,
    mut retired: RemovedComponents<CapturedJob<J>>,
    wake: Option<Res<JobWake>>,
) {
    let mut drained = false;
    for _ in retired.read() {
        drained = true;
    }
    if (!ready.is_empty() || drained)
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}

struct WorkerPanic(Box<dyn Any + Send>);

impl WorkerPanic {
    fn message(&self) -> String {
        if let Some(message) = self.0.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = self.0.downcast_ref::<&str>() {
            return message.to_string();
        }
        "blocking job panicked".to_string()
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use crate::blocking::BlockingPool;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    struct ChannelWake {
        output: mpsc::SyncSender<()>,
        source: JobWake,
    }

    impl std::task::Wake for ChannelWake {
        fn wake(self: Arc<Self>) {
            self.source.0.wake_by_ref();
            let _ = self.output.try_send(());
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.source.0.wake_by_ref();
            let _ = self.output.try_send(());
        }
    }

    #[test]
    fn native_job_completion_wakes_result_consumers_and_canceled_owner_cleanup() {
        for canceled in [false, true] {
            let mut fixture = Fixture::new();
            let (native_wake, native_events) = mpsc::sync_channel(16);
            fixture
                .app
                .insert_resource(JobWake(std::task::Waker::from(Arc::new(ChannelWake {
                    output: native_wake,
                    source: JobWake(std::task::Waker::noop().clone()),
                }))));
            fixture.start();
            if canceled {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.operation)
                    .insert(OperationState::Canceled);
                fixture.app.update();
            }
            while native_events.try_recv().is_ok() {}
            fixture.release.send(()).unwrap();
            native_events.recv_timeout(Duration::from_secs(3)).unwrap();
            let (collection_wake, collection_events) = mpsc::sync_channel(16);
            let source = fixture.app.world().resource::<JobWake>().clone();
            fixture
                .app
                .insert_resource(JobWake(std::task::Waker::from(Arc::new(ChannelWake {
                    output: collection_wake,
                    source,
                }))));
            fixture.app.update();
            assert!(
                collection_events.try_recv().is_ok(),
                "result delivery and native drain must wake the next ECS frame; canceled={canceled}"
            );
            if canceled {
                assert!(fixture.cleaned.load(Ordering::Acquire));
                assert!(!fixture.running());
            } else {
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<JobResult<WriteProbe>>(fixture.operation)
                        .is_some()
                );
            }
        }
    }

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        owner: Entity,
        operation: Entity,
        release: mpsc::SyncSender<()>,
        started: mpsc::Receiver<()>,
        cleaned: Arc<AtomicBool>,
        cancellation_seen: Arc<AtomicBool>,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.insert_resource(BlockingRuntime::new(
                BlockingPool::new("job-drain", 1, 4),
                BlockingPool::new("unused", 0, 0),
                BlockingPool::new("unused", 0, 0),
            ))
            .add_plugins(JobPlugin::<WriteProbe>::default());
            let owner = app.world_mut().spawn_empty().id();
            let (started_tx, started) = mpsc::sync_channel(1);
            let (release, release_rx) = mpsc::sync_channel(1);
            let cleaned = Arc::new(AtomicBool::new(false));
            let cancellation_seen = Arc::new(AtomicBool::new(false));
            let operation = app
                .world_mut()
                .spawn((
                    Operation::bundle("test-write").unwrap(),
                    ChildOf(owner),
                    JobRequest(WriteProbe {
                        path: directory.path().join("written"),
                        started: started_tx,
                        release: Arc::new(Mutex::new(release_rx)),
                        cleaned: Arc::clone(&cleaned),
                        cancellation_seen: Arc::clone(&cancellation_seen),
                    }),
                ))
                .id();
            Self {
                app,
                directory,
                owner,
                operation,
                release,
                started,
                cleaned,
                cancellation_seen,
            }
        }

        fn start(&mut self) {
            self.app.update();
            self.started.recv_timeout(Duration::from_secs(3)).unwrap();
        }

        fn running(&mut self) -> bool {
            let world = self.app.world_mut();
            world
                .query_filtered::<&OperationState, With<Operation>>()
                .iter(world)
                .any(|state| *state == OperationState::Running)
        }

        fn finish(&mut self) {
            self.release.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                self.app.update();
                if self.cleaned.load(Ordering::Acquire) && !self.running() {
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                std::fs::read(self.directory.path().join("written")).unwrap(),
                b"native write"
            );
            assert!(
                self.app
                    .world()
                    .get::<JobResult<WriteProbe>>(self.operation)
                    .is_none()
            );
        }
    }

    #[derive(Clone)]
    struct WriteProbe {
        path: PathBuf,
        started: mpsc::SyncSender<()>,
        release: Arc<Mutex<mpsc::Receiver<()>>>,
        cleaned: Arc<AtomicBool>,
        cancellation_seen: Arc<AtomicBool>,
    }

    impl Job for WriteProbe {
        type Output = Output;

        fn run(self) -> Self::Output {
            self.started.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            self.cancellation_seen.store(
                crate::blocking::BlockingCancellation::canceled(),
                Ordering::Release,
            );
            let mut file = File::create(self.path).unwrap();
            file.write_all(b"native write").unwrap();
            Output {
                file: Some(file),
                cleaned: self.cleaned,
            }
        }
    }

    struct Output {
        file: Option<File>,
        cleaned: Arc<AtomicBool>,
    }

    impl Drop for Output {
        fn drop(&mut self) {
            drop(self.file.take());
            self.cleaned.store(true, Ordering::Release);
        }
    }

    #[test]
    fn canceled_request_keeps_native_ownership_until_io_and_output_cleanup_finish() {
        let mut fixture = Fixture::new();
        fixture.start();
        *fixture
            .app
            .world_mut()
            .get_mut::<OperationState>(fixture.operation)
            .unwrap() = OperationState::Canceled;
        fixture.app.update();
        assert!(fixture.running());
        assert!(!fixture.cleaned.load(Ordering::Acquire));
        assert!(!fixture.directory.path().join("written").exists());
        fixture.finish();
    }

    #[test]
    fn deleting_request_owner_does_not_drop_unfinished_native_io() {
        let mut fixture = Fixture::new();
        fixture.start();
        fixture.app.world_mut().entity_mut(fixture.owner).despawn();
        fixture.app.update();
        assert!(fixture.app.world().get_entity(fixture.operation).is_err());
        assert!(fixture.running());
        assert!(!fixture.cleaned.load(Ordering::Acquire));
        fixture.finish();
    }

    #[test]
    fn reparented_request_cannot_receive_old_native_result() {
        let mut fixture = Fixture::new();
        fixture.start();
        let other = fixture.app.world_mut().spawn_empty().id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.operation)
            .insert(ChildOf(other));
        fixture.app.update();
        assert!(fixture.running());
        assert_eq!(
            fixture.app.world().get::<OperationState>(fixture.operation),
            Some(&OperationState::Canceled)
        );
        fixture.finish();
    }

    #[test]
    fn a_full_native_queue_keeps_request_pending_and_cancelable() {
        let mut fixture = Fixture::new();
        fixture.app.insert_resource(BlockingRuntime::new(
            BlockingPool::new("queue-pressure", 1, 1),
            BlockingPool::new("unused", 0, 0),
            BlockingPool::new("unused", 0, 0),
        ));
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let runtime = fixture.app.world().resource::<BlockingRuntime>();
        let first = runtime
            .spawn(None, move || {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let second = runtime.spawn(None, || ()).unwrap();
        let (drain_wake, drained) = mpsc::sync_channel(16);
        fixture
            .app
            .insert_resource(JobWake(std::task::Waker::from(Arc::new(ChannelWake {
                output: drain_wake,
                source: JobWake(std::task::Waker::noop().clone()),
            }))));
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(fixture.operation),
            Some(&OperationState::Pending)
        );
        *fixture
            .app
            .world_mut()
            .get_mut::<OperationState>(fixture.operation)
            .unwrap() = OperationState::Canceled;
        fixture.app.world_mut().run_schedule(PostUpdate);
        assert!(
            drained.try_recv().is_ok(),
            "canceling an unstarted native job must wake owner cleanup"
        );
        assert!(!fixture.running());
        let world = fixture.app.world_mut();
        assert!(
            world
                .query_filtered::<&OperationState, With<Operation>>()
                .iter(world)
                .all(|state| !matches!(state, OperationState::Pending | OperationState::Running))
        );
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !first.is_finished() || !second.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(matches!(
            fixture.started.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        assert!(!fixture.directory.path().join("written").exists());
    }

    #[test]
    fn late_frame_cancellation_signals_native_io_without_another_update() {
        for remove_owner in [false, true] {
            let mut fixture = Fixture::new();
            fixture.start();
            if remove_owner {
                fixture.app.world_mut().entity_mut(fixture.owner).despawn();
            } else {
                *fixture
                    .app
                    .world_mut()
                    .get_mut::<OperationState>(fixture.operation)
                    .unwrap() = OperationState::Canceled;
            }
            fixture.app.world_mut().run_schedule(PostUpdate);
            assert!(fixture.running());
            fixture.release.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !fixture.cleaned.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            assert!(fixture.cancellation_seen.load(Ordering::Acquire));
            fixture.app.update();
            assert!(!fixture.running());
        }
    }

    #[test]
    fn inserting_job_wake_synchronizes_channel_wake() {
        let mut app = App::new();
        let (wake_tx, wake_rx) = mpsc::sync_channel(4);
        app.insert_resource(JobWake(Waker::from(Arc::new(ChannelWake {
            output: wake_tx,
            source: JobWake(Waker::noop().clone()),
        }))));
        let channel_wake = app
            .world()
            .resource::<crate::channel::ChannelWake>()
            .clone();
        channel_wake.0.wake_by_ref();
        assert!(wake_rx.try_recv().is_ok());
    }

    #[test]
    fn completed_native_result_remains_addressed_to_the_live_request() {
        let mut fixture = Fixture::new();
        fixture.start();
        fixture.release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            fixture.app.update();
            if fixture
                .app
                .world()
                .get::<JobResult<WriteProbe>>(fixture.operation)
                .is_some()
            {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            std::fs::read(fixture.directory.path().join("written")).unwrap(),
            b"native write"
        );
        assert!(!fixture.cleaned.load(Ordering::Acquire));
        *fixture
            .app
            .world_mut()
            .get_mut::<OperationState>(fixture.operation)
            .unwrap() = OperationState::Succeeded;
        fixture.app.update();
        assert!(fixture.cleaned.load(Ordering::Acquire));
        assert!(!fixture.running());
    }
}
