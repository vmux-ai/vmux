use std::future::pending;
use std::sync::Arc;
use std::task::{Wake, Waker};
use std::time::{Duration, Instant};

use bevy_app::{App, AppExit, PluginsState};
use bevy_ecs::prelude::{Entity, With};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use vmux_ecs::channel::ChannelWake;
use vmux_ecs::time::TimeWake;
use vmux_ecs::{
    JobWake, Shutdown, ShutdownBarrier, ShutdownComplete, ShutdownPending, ShutdownPlugin,
    ShutdownState, ShutdownTimeout, TaskWake,
};
use vmux_task::TokioTask;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParkOutcome {
    Woken,
    TimedOut,
    Shutdown,
}

#[derive(Clone)]
pub struct RunnerHandle {
    wake_tx: mpsc::UnboundedSender<()>,
    shutdown_tx: mpsc::Sender<()>,
}

impl RunnerHandle {
    pub fn wake(&self) {
        let _ = self.wake_tx.send(());
    }

    pub fn waker(&self) -> Waker {
        Waker::from(Arc::new(ChannelWaker(self.wake_tx.clone())))
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.try_send(());
    }

    pub async fn shutdown_async(&self) {
        let _ = self.shutdown_tx.send(()).await;
    }

    pub fn wake_sender(&self) -> mpsc::UnboundedSender<()> {
        self.wake_tx.clone()
    }

    pub fn shutdown_sender(&self) -> mpsc::Sender<()> {
        self.shutdown_tx.clone()
    }
}

struct ChannelWaker(mpsc::UnboundedSender<()>);

impl Wake for ChannelWaker {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(());
    }
}

struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub struct HeadlessTokioRunner {
    runtime: Handle,
    internal_wake_tx: mpsc::UnboundedSender<()>,
    internal_wake_rx: mpsc::UnboundedReceiver<()>,
    external_wake_rx: Option<mpsc::UnboundedReceiver<()>>,
    internal_shutdown_tx: mpsc::Sender<()>,
    internal_shutdown_rx: mpsc::Receiver<()>,
    external_shutdown_rx: Option<mpsc::Receiver<()>>,
    housekeeping_floor: Duration,
    shutdown_timeout: Option<Duration>,
    os_signals: bool,
}

impl HeadlessTokioRunner {
    pub const DEFAULT_HOUSEKEEPING_FLOOR: Duration = Duration::from_secs(1);
    const DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(5);

    pub fn new(runtime: Handle) -> Self {
        let (internal_wake_tx, internal_wake_rx) = mpsc::unbounded_channel();
        let (internal_shutdown_tx, internal_shutdown_rx) = mpsc::channel(1);
        Self {
            runtime,
            internal_wake_tx,
            internal_wake_rx,
            external_wake_rx: None,
            internal_shutdown_tx,
            internal_shutdown_rx,
            external_shutdown_rx: None,
            housekeeping_floor: Self::DEFAULT_HOUSEKEEPING_FLOOR,
            shutdown_timeout: None,
            os_signals: false,
        }
    }

    pub fn handles(runtime: Handle) -> (RunnerHandle, Self) {
        let runner = Self::new(runtime);
        let handle = runner.handle();
        (handle, runner)
    }

    pub fn handle(&self) -> RunnerHandle {
        RunnerHandle {
            wake_tx: self.internal_wake_tx.clone(),
            shutdown_tx: self.internal_shutdown_tx.clone(),
        }
    }

    pub fn with_wake_channel(mut self, wake_rx: mpsc::UnboundedReceiver<()>) -> Self {
        self.external_wake_rx = Some(wake_rx);
        self
    }

    pub fn with_shutdown_channel(mut self, shutdown_rx: mpsc::Receiver<()>) -> Self {
        self.external_shutdown_rx = Some(shutdown_rx);
        self
    }

    pub fn with_housekeeping_floor(mut self, floor: Duration) -> Self {
        self.housekeeping_floor = floor;
        self
    }

    pub fn with_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = Some(timeout);
        self
    }

    pub fn with_os_signals(mut self) -> Self {
        self.os_signals = true;
        self
    }

    pub fn into_runner(mut self) -> impl FnOnce(App) -> AppExit {
        move |mut app: App| self.run(&mut app)
    }

    pub fn run(&mut self, app: &mut App) -> AppExit {
        self.prepare_app(app);
        let _signals = self.install_os_signals();
        while app.plugins_state() == PluginsState::Adding {
            bevy_tasks::tick_global_task_pools_on_main_thread();
        }
        if app.plugins_state() == PluginsState::Ready {
            app.finish();
        }
        if app.plugins_state() == PluginsState::Finished {
            app.cleanup();
        }
        loop {
            app.update();
            if let Some(exit) = app.should_exit() {
                return self.finish(app, exit);
            }
            if self.park(self.housekeeping_floor) == ParkOutcome::Shutdown {
                return self.finish(app, AppExit::Success);
            }
        }
    }

    pub fn park(&mut self, floor: Duration) -> ParkOutcome {
        let outcome = self.runtime.block_on(async {
            tokio::select! {
                biased;
                _ = self.internal_shutdown_rx.recv() => ParkOutcome::Shutdown,
                _ = Self::recv_shutdown(&mut self.external_shutdown_rx) => ParkOutcome::Shutdown,
                wake = Self::recv_wake(&mut self.external_wake_rx) => match wake {
                    Some(()) => ParkOutcome::Woken,
                    None => ParkOutcome::Shutdown,
                },
                wake = self.internal_wake_rx.recv() => match wake {
                    Some(()) => ParkOutcome::Woken,
                    None => ParkOutcome::Shutdown,
                },
                _ = tokio::time::sleep(floor) => ParkOutcome::TimedOut,
            }
        });
        if outcome == ParkOutcome::Woken {
            self.drain_wakes();
        }
        outcome
    }

    pub fn try_recv_wake(&mut self) -> bool {
        let mut received = false;
        if self.internal_wake_rx.try_recv().is_ok() {
            received = true;
        }
        if let Some(external) = &mut self.external_wake_rx
            && external.try_recv().is_ok()
        {
            received = true;
        }
        received
    }

    fn prepare_app(&self, app: &mut App) {
        if !app.world().contains_resource::<ShutdownState>()
            && matches!(
                app.plugins_state(),
                PluginsState::Adding | PluginsState::Ready
            )
        {
            app.add_plugins(ShutdownPlugin);
        }
        if let Some(timeout) = self.shutdown_timeout {
            app.insert_resource(ShutdownTimeout::new(timeout));
        }
        let waker = self.handle().waker();
        if !app.world().contains_resource::<JobWake>() {
            app.insert_resource(JobWake(waker.clone()));
        }
        if !app.world().contains_resource::<ChannelWake>() {
            app.insert_resource(ChannelWake(waker.clone()));
        }
        if !app.world().contains_resource::<TaskWake>() {
            app.insert_resource(TaskWake(waker.clone()));
        }
        if !app.world().contains_resource::<TimeWake>() {
            app.insert_resource(TimeWake(waker));
        }
    }

    fn install_os_signals(&self) -> Option<AbortOnDrop> {
        if !self.os_signals {
            return None;
        }
        let shutdown_tx = self.internal_shutdown_tx.clone();
        let task = self.runtime.spawn(async move {
            Self::wait_os_signal().await;
            let _ = shutdown_tx.send(()).await;
        });
        Some(AbortOnDrop(task))
    }

    #[cfg(unix)]
    async fn wait_os_signal() {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sigterm) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = sigterm.recv() => {}
                }
            }
            Err(error) => {
                tracing::warn!(%error, "failed to install SIGTERM handler, waiting for Ctrl-C");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }

    #[cfg(not(unix))]
    async fn wait_os_signal() {
        let _ = tokio::signal::ctrl_c().await;
    }

    fn finish(&mut self, app: &mut App, exit: AppExit) -> AppExit {
        let exit = if let Some(mut state) = app.world_mut().get_resource_mut::<ShutdownState>() {
            state.begin(exit.clone());
            state.exit().unwrap_or(exit)
        } else {
            exit
        };
        if let Err(error) = app.world_mut().try_run_schedule(Shutdown) {
            tracing::debug!(%error, "no shutdown schedule participants");
        }
        let timeout = self
            .shutdown_timeout
            .or_else(|| {
                app.world()
                    .get_resource::<ShutdownTimeout>()
                    .map(|timeout| timeout.duration())
            })
            .unwrap_or(ShutdownTimeout::DEFAULT);
        let deadline = Instant::now() + timeout;
        while Self::has_pending_shutdown(app) {
            let now = Instant::now();
            if now >= deadline {
                tracing::warn!("shutdown drain timed out before all barriers and tasks completed");
                break;
            }
            app.update();
            if !Self::has_pending_shutdown(app) {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            self.park_drain(remaining.min(Self::DRAIN_POLL_INTERVAL));
        }
        if let Some(mut state) = app.world_mut().get_resource_mut::<ShutdownState>() {
            state.finish();
        }
        if let Err(error) = app.world_mut().try_run_schedule(ShutdownComplete) {
            tracing::debug!(%error, "no final shutdown schedule participants");
        }
        exit
    }

    fn has_pending_shutdown(app: &mut App) -> bool {
        let world = app.world_mut();
        let has_barrier = world
            .query_filtered::<Entity, With<ShutdownBarrier>>()
            .iter(world)
            .next()
            .is_some();
        if has_barrier {
            return true;
        }
        let has_tokio_task = world
            .query_filtered::<&TokioTask, With<ShutdownPending>>()
            .iter(world)
            .any(|task| !task.is_finished());
        if has_tokio_task {
            return true;
        }
        #[cfg(all(not(target_os = "ios"), feature = "host"))]
        {
            let has_abort_task = world
                .query_filtered::<&vmux_ecs::service::AbortTask, With<ShutdownPending>>()
                .iter(world)
                .any(|task| !task.0.is_finished());
            if has_abort_task {
                return true;
            }
        }
        false
    }

    fn park_drain(&mut self, wait: Duration) {
        self.runtime.block_on(async {
            tokio::select! {
                _ = self.internal_wake_rx.recv() => {}
                Some(()) = Self::recv_wake(&mut self.external_wake_rx) => {}
                _ = tokio::time::sleep(wait) => {}
            }
        });
        self.drain_wakes();
    }

    fn drain_wakes(&mut self) {
        while self.internal_wake_rx.try_recv().is_ok() {}
        if let Some(external) = &mut self.external_wake_rx {
            while external.try_recv().is_ok() {}
        }
    }

    async fn recv_wake(rx: &mut Option<mpsc::UnboundedReceiver<()>>) -> Option<()> {
        match rx {
            Some(rx) => rx.recv().await,
            None => pending().await,
        }
    }

    async fn recv_shutdown(rx: &mut Option<mpsc::Receiver<()>>) -> Option<()> {
        match rx {
            Some(rx) => rx.recv().await,
            None => pending().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use bevy_app::{PostUpdate, Update};
    use bevy_ecs::prelude::*;
    use vmux_ecs::channel::{AsyncChannelPlugin, MessageSender};
    use vmux_ecs::time::{Clock, Deadline, DeadlinePlugin, DeadlineReached};
    use vmux_ecs::{IoTask, IoTaskPlugin, Job, JobPlugin, JobRequest, JobResult, ShutdownPhase};
    use vmux_ecs::{Operation, ShutdownRequest};

    use super::*;

    fn test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn burst_wakes_coalesce_into_one_park_return() {
        let rt = test_runtime();
        let (handle, mut runner) = HeadlessTokioRunner::handles(rt.handle().clone());
        for _ in 0..8 {
            handle.wake();
        }
        assert_eq!(runner.park(Duration::from_secs(30)), ParkOutcome::Woken);
        assert!(!runner.try_recv_wake());
    }

    #[test]
    fn idle_runner_wakes_on_housekeeping_floor() {
        let rt = test_runtime();
        let (_handle, mut runner) = HeadlessTokioRunner::handles(rt.handle().clone());
        assert_eq!(
            runner.park(Duration::from_millis(10)),
            ParkOutcome::TimedOut
        );
    }

    #[derive(Resource)]
    struct LifecycleLog(Arc<Mutex<Vec<&'static str>>>);

    #[derive(Resource)]
    struct BarrierRelease {
        tx: Option<tokio::sync::oneshot::Sender<()>>,
        rx: Option<tokio::sync::oneshot::Receiver<()>>,
    }

    #[derive(Component)]
    struct DrainEntity;

    fn begin_barrier_drain(
        mut commands: Commands,
        mut release: ResMut<BarrierRelease>,
        state: Res<ShutdownState>,
        log: Res<LifecycleLog>,
    ) {
        assert_eq!(state.phase(), ShutdownPhase::Draining);
        log.0.lock().unwrap().push("shutdown");
        commands.spawn((DrainEntity, ShutdownBarrier));
        if let Some(tx) = release.tx.take() {
            let _ = tx.send(());
        }
    }

    fn poll_barrier_release(
        mut commands: Commands,
        mut release: ResMut<BarrierRelease>,
        barriers: Query<Entity, With<DrainEntity>>,
        log: Res<LifecycleLog>,
    ) {
        let Some(rx) = release.rx.as_mut() else {
            return;
        };
        if rx.try_recv().is_ok() {
            release.rx = None;
            log.0.lock().unwrap().push("barrier_released");
            for entity in &barriers {
                commands.entity(entity).remove::<ShutdownBarrier>();
            }
        }
    }

    fn record_complete(state: Res<ShutdownState>, log: Res<LifecycleLog>) {
        assert_eq!(state.phase(), ShutdownPhase::Complete);
        log.0.lock().unwrap().push("complete");
    }

    #[test]
    fn shutdown_runs_schedule_drains_barrier_and_tokio_task_then_runs_complete() {
        let rt = test_runtime();
        let (handle, mut runner) = HeadlessTokioRunner::handles(rt.handle().clone());
        runner = runner.with_housekeeping_floor(Duration::from_secs(30));
        let log = Arc::new(Mutex::new(Vec::new()));
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let task_finished = Arc::new(AtomicBool::new(false));
        let task_flag = Arc::clone(&task_finished);
        let wake_handle = handle.clone();
        let task = TokioTask::spawn(rt.handle(), async move {
            start_rx.await.unwrap();
            tokio::time::sleep(Duration::from_millis(15)).await;
            let _ = done_tx.send(());
            wake_handle.wake();
            tokio::time::sleep(Duration::from_millis(15)).await;
            task_flag.store(true, Ordering::SeqCst);
        });

        let mut app = App::new();
        app.add_plugins(ShutdownPlugin)
            .insert_resource(LifecycleLog(Arc::clone(&log)))
            .insert_resource(BarrierRelease {
                tx: Some(start_tx),
                rx: Some(done_rx),
            })
            .add_systems(Shutdown, begin_barrier_drain)
            .add_systems(Update, poll_barrier_release)
            .add_systems(ShutdownComplete, record_complete);
        app.world_mut().spawn((task, ShutdownPending));

        handle.shutdown();
        let exit = runner.run(&mut app);
        assert_eq!(exit, AppExit::Success);
        assert!(task_finished.load(Ordering::SeqCst));
        assert_eq!(
            *log.lock().unwrap(),
            vec!["shutdown", "barrier_released", "complete"]
        );
        assert!(app.world().resource::<ShutdownState>().is_complete());
    }

    #[test]
    fn shutdown_timeout_bounds_stuck_barrier() {
        let rt = test_runtime();
        let (handle, mut runner) = HeadlessTokioRunner::handles(rt.handle().clone());
        runner = runner.with_shutdown_timeout(Duration::from_millis(40));
        let mut app = App::new();
        app.add_plugins(ShutdownPlugin);
        app.world_mut().spawn(ShutdownBarrier);
        handle.shutdown();

        let started = Instant::now();
        let exit = runner.run(&mut app);
        assert_eq!(exit, AppExit::Success);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(app.world().resource::<ShutdownState>().is_complete());
    }

    #[derive(Message, Clone)]
    struct Ping(u32);

    struct TickTag;

    #[derive(Component)]
    struct LoadedValue(u32);

    #[derive(Clone)]
    struct ComputeJob(u32);

    impl Job for ComputeJob {
        type Output = u32;

        fn run(self) -> Self::Output {
            self.0 * 2
        }
    }

    #[derive(Resource, Default)]
    struct IntegrationState {
        ping: Option<u32>,
        deadline_fired: bool,
        io_value: Option<u32>,
        job_value: Option<u32>,
        updates: Arc<AtomicUsize>,
    }

    fn on_ping(
        mut reader: MessageReader<Ping>,
        mut state: ResMut<IntegrationState>,
        mut commands: Commands,
    ) {
        for ping in reader.read() {
            state.ping = Some(ping.0);
            commands.spawn(IoTask::<LoadedValue>::spawn(async { Ok(LoadedValue(42)) }));
            commands.spawn((
                Operation::bundle("runner-job").unwrap(),
                JobRequest(ComputeJob(21)),
            ));
        }
    }

    fn on_deadline(_: On<DeadlineReached<TickTag>>, mut state: ResMut<IntegrationState>) {
        state.deadline_fired = true;
    }

    fn collect_io_and_jobs(
        io: Query<&LoadedValue>,
        jobs: Query<&JobResult<ComputeJob>>,
        mut state: ResMut<IntegrationState>,
        mut shutdown: MessageWriter<ShutdownRequest>,
    ) {
        state.updates.fetch_add(1, Ordering::SeqCst);
        for value in &io {
            state.io_value = Some(value.0);
        }
        for result in &jobs {
            if let Ok(value) = result.0 {
                state.job_value = Some(value);
            }
        }
        if state.ping == Some(7)
            && state.deadline_fired
            && state.io_value == Some(42)
            && state.job_value == Some(42)
        {
            shutdown.write(ShutdownRequest::error(NonZero::new(3).unwrap()));
        }
    }

    #[test]
    fn automatic_wake_wiring_drives_channels_deadlines_io_tasks_and_jobs() {
        let rt = test_runtime();
        let updates = Arc::new(AtomicUsize::new(0));
        let mut runner = HeadlessTokioRunner::new(rt.handle().clone())
            .with_housekeeping_floor(Duration::from_secs(30));

        let mut app = App::new();
        app.add_plugins((
            ShutdownPlugin,
            AsyncChannelPlugin::message::<Ping>(),
            DeadlinePlugin::<TickTag>::default(),
            IoTaskPlugin::<(LoadedValue,)>::default(),
            JobPlugin::<ComputeJob>::default(),
        ))
        .insert_resource(IntegrationState {
            updates: Arc::clone(&updates),
            ..Default::default()
        })
        .add_observer(on_deadline)
        .add_systems(Update, on_ping)
        .add_systems(PostUpdate, collect_io_and_jobs);

        let clock = app.world().resource::<Clock>().clone();
        app.world_mut().spawn(Deadline::<TickTag>::after(
            &clock,
            Duration::from_millis(20),
        ));
        let sender = app.world().resource::<MessageSender<Ping>>().clone();
        rt.spawn(async move {
            tokio::time::sleep(Duration::from_millis(25)).await;
            sender.send(Ping(7)).unwrap();
        });

        let started = Instant::now();
        let exit = runner.run(&mut app);
        assert_eq!(exit, AppExit::Error(NonZero::new(3).unwrap()));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "automatic wakes must complete well before the 30s housekeeping floor"
        );
        let state = app.world().resource::<IntegrationState>();
        assert_eq!(state.ping, Some(7));
        assert!(state.deadline_fired);
        assert_eq!(state.io_value, Some(42));
        assert_eq!(state.job_value, Some(42));
        assert!(updates.load(Ordering::SeqCst) >= 2);
    }
}
