use bevy_app::prelude::*;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use vmux_runner::HeadlessTokioRunner;

pub struct WakeDrivenRunner {
    inner: HeadlessTokioRunner,
}

impl WakeDrivenRunner {
    pub fn new(
        runtime: Handle,
        wake_rx: mpsc::UnboundedReceiver<()>,
        signal_rx: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            inner: HeadlessTokioRunner::new(runtime)
                .with_wake_channel(wake_rx)
                .with_shutdown_channel(signal_rx),
        }
    }

    pub fn into_runner(self) -> impl FnOnce(App) -> AppExit {
        self.inner.into_runner()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use vmux_runner::ParkOutcome;

    use super::*;

    #[derive(bevy_ecs::prelude::Resource)]
    struct BeginDrain(Option<tokio::sync::oneshot::Sender<()>>);

    impl BeginDrain {
        fn begin(mut state: bevy_ecs::prelude::ResMut<Self>) {
            state.0.take().unwrap().send(()).unwrap();
        }
    }

    #[test]
    fn shutdown_keeps_processing_ecs_requests_until_the_closing_task_finishes() {
        let rt = WakeDrivenRunner::test_runtime();
        let (wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (signal_tx, signal_rx) = mpsc::channel(1);
        signal_tx.try_send(()).unwrap();
        let (processes, inboxes) = vmux_process::ProcessRuntime::new(wake_tx);
        let finalized = Arc::new(AtomicBool::new(false));
        let done = finalized.clone();
        let (begin, started) = tokio::sync::oneshot::channel();
        let task = rt.spawn(async move {
            started.await.unwrap();
            assert!(processes.list().await.unwrap().is_empty());
            done.store(true, Ordering::SeqCst);
        });
        let mut app = App::new();
        app.add_plugins(vmux_process::ProcessPlugin)
            .insert_resource(BeginDrain(Some(begin)))
            .add_systems(vmux_ecs::service::Shutdown, BeginDrain::begin)
            .set_runner(
                WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx).into_runner(),
            );
        app.world_mut().spawn(inboxes);
        app.world_mut().spawn((
            vmux_ecs::TokioTask::new(task),
            vmux_ecs::service::ShutdownPending,
        ));
        assert_eq!(app.run(), AppExit::Success);
        assert!(finalized.load(Ordering::SeqCst));
    }

    #[derive(bevy_ecs::prelude::Resource)]
    struct Finalized(Arc<AtomicBool>);

    impl Finalized {
        fn finish(state: bevy_ecs::prelude::Res<Self>) {
            state.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn shutdown_signal_runs_registered_cleanup_before_dropping_the_application() {
        let rt = WakeDrivenRunner::test_runtime();
        let (_wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (signal_tx, signal_rx) = mpsc::channel(1);
        signal_tx.try_send(()).unwrap();
        let finalized = Arc::new(AtomicBool::new(false));
        let mut app = App::new();
        app.insert_resource(Finalized(finalized.clone()))
            .add_systems(vmux_ecs::service::Shutdown, Finalized::finish)
            .set_runner(
                WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx).into_runner(),
            );
        assert_eq!(app.run(), AppExit::Success);
        assert!(finalized.load(Ordering::SeqCst));
    }

    struct Feature;

    impl vmux_feature::manifest::FeatureManifestSource for Feature {
        const SOURCE: &'static str =
            "(commands: [(id: \"runner.check\", label: \"Check\", group: \"File\")])";
    }

    #[derive(bevy_ecs::prelude::Resource)]
    struct Frozen(Arc<AtomicBool>);

    impl Frozen {
        fn check(
            registry: Option<bevy_ecs::prelude::Res<vmux_feature::manifest::Registry>>,
            frozen: bevy_ecs::prelude::Res<Self>,
            mut exits: bevy_ecs::prelude::MessageWriter<AppExit>,
        ) {
            assert!(registry.is_some_and(|registry| registry.manifest::<Feature>().is_some()));
            frozen.0.store(true, Ordering::SeqCst);
            exits.write(AppExit::Success);
        }
    }

    #[test]
    fn feature_registry_is_frozen_before_the_first_runner_update() {
        let rt = WakeDrivenRunner::test_runtime();
        let (_wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (_signal_tx, signal_rx) = mpsc::channel(1);
        let frozen = Arc::new(AtomicBool::new(false));
        let mut app = App::new();
        app.add_plugins(vmux_feature::manifest::FeaturePlugin::<Feature>::default())
            .insert_resource(Frozen(frozen.clone()))
            .add_systems(Update, Frozen::check)
            .set_runner(
                WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx).into_runner(),
            );
        assert_eq!(app.run(), AppExit::Success);
        assert!(frozen.load(Ordering::SeqCst));
    }

    impl WakeDrivenRunner {
        fn test_runtime() -> tokio::runtime::Runtime {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap()
        }
    }

    #[test]
    fn a_burst_of_wakes_costs_one_update() {
        let rt = WakeDrivenRunner::test_runtime();
        let (wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (_signal_tx, signal_rx) = mpsc::channel(1);
        for _ in 0..5 {
            wake_tx.send(()).unwrap();
        }

        let mut runner = WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx);
        let outcome = runner.inner.park(Duration::from_secs(30));

        assert_eq!(outcome, ParkOutcome::Woken);
        assert!(
            !runner.inner.try_recv_wake(),
            "the other four wakes should have been coalesced into this update"
        );
    }

    #[test]
    fn an_idle_daemon_wakes_only_on_the_housekeeping_floor() {
        let rt = WakeDrivenRunner::test_runtime();
        let (_wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (_signal_tx, signal_rx) = mpsc::channel(1);

        let mut runner = WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx);
        let outcome = runner.inner.park(Duration::from_millis(10));

        assert_eq!(outcome, ParkOutcome::TimedOut);
    }

    #[test]
    fn a_signal_stops_the_runner() {
        let rt = WakeDrivenRunner::test_runtime();
        let (_wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (signal_tx, signal_rx) = mpsc::channel(1);
        signal_tx.try_send(()).unwrap();

        let mut runner = WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx);
        let outcome = runner.inner.park(Duration::from_secs(30));

        assert_eq!(outcome, ParkOutcome::Shutdown);
    }

    #[test]
    fn the_server_dropping_its_wake_sender_stops_the_runner() {
        let rt = WakeDrivenRunner::test_runtime();
        let (wake_tx, wake_rx) = mpsc::unbounded_channel::<()>();
        let (_signal_tx, signal_rx) = mpsc::channel(1);
        drop(wake_tx);

        let mut runner = WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx);
        let outcome = runner.inner.park(Duration::from_secs(30));

        assert_eq!(outcome, ParkOutcome::Shutdown);
    }

    #[test]
    fn the_app_exits_when_a_signal_arrives() {
        let rt = WakeDrivenRunner::test_runtime();
        let (wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (signal_tx, signal_rx) = mpsc::channel(1);
        signal_tx.try_send(()).unwrap();

        let mut app = App::new();
        app.set_runner(
            WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx).into_runner(),
        );

        assert_eq!(app.run(), AppExit::Success);
        drop(wake_tx);
    }
}
