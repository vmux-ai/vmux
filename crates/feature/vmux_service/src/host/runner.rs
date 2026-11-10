use std::time::Duration;

use bevy_app::prelude::*;
use tokio::runtime::Handle;
use tokio::sync::mpsc;

const HOUSEKEEPING_FLOOR: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParkOutcome {
    Woken,
    TimedOut,
    Shutdown,
}

pub struct WakeDrivenRunner {
    runtime: Handle,
    wake_rx: mpsc::UnboundedReceiver<()>,
    signal_rx: mpsc::Receiver<()>,
}

impl WakeDrivenRunner {
    pub fn new(
        runtime: Handle,
        wake_rx: mpsc::UnboundedReceiver<()>,
        signal_rx: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            runtime,
            wake_rx,
            signal_rx,
        }
    }

    pub fn into_runner(mut self) -> impl FnOnce(App) -> AppExit {
        move |mut app: App| self.run(&mut app)
    }

    fn run(&mut self, app: &mut App) -> AppExit {
        while app.plugins_state() == bevy_app::PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        if app.plugins_state() == bevy_app::PluginsState::Ready {
            app.finish();
        }
        if app.plugins_state() == bevy_app::PluginsState::Finished {
            app.cleanup();
        }
        loop {
            app.update();
            if let Some(exit) = app.should_exit() {
                return exit;
            }
            if self.park(HOUSEKEEPING_FLOOR) == ParkOutcome::Shutdown {
                return AppExit::Success;
            }
        }
    }

    fn park(&mut self, floor: Duration) -> ParkOutcome {
        let outcome = self.runtime.block_on(async {
            tokio::select! {
                wake = self.wake_rx.recv() => match wake {
                    Some(_) => ParkOutcome::Woken,
                    None => ParkOutcome::Shutdown,
                },
                _ = self.signal_rx.recv() => ParkOutcome::Shutdown,
                _ = tokio::time::sleep(floor) => ParkOutcome::TimedOut,
            }
        });

        if outcome == ParkOutcome::Woken {
            while self.wake_rx.try_recv().is_ok() {}
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Feature;

    impl vmux_ecs::manifest::FeatureManifestSource for Feature {
        const SOURCE: &'static str =
            "(commands: [(id: \"runner.check\", label: \"Check\", group: \"File\")])";
    }

    #[derive(bevy_ecs::prelude::Resource)]
    struct Frozen(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl Frozen {
        fn check(
            registry: Option<bevy_ecs::prelude::Res<vmux_ecs::manifest::Registry>>,
            frozen: bevy_ecs::prelude::Res<Self>,
            mut exits: bevy_ecs::prelude::MessageWriter<AppExit>,
        ) {
            assert!(registry.is_some_and(|registry| registry.manifest::<Feature>().is_some()));
            frozen.0.store(true, std::sync::atomic::Ordering::SeqCst);
            exits.write(AppExit::Success);
        }
    }

    #[test]
    fn feature_registry_is_frozen_before_the_first_runner_update() {
        let rt = runtime();
        let (_wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (_signal_tx, signal_rx) = mpsc::channel(1);
        let frozen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut app = App::new();
        app.add_plugins(vmux_ecs::manifest::FeaturePlugin::<Feature>::default())
            .insert_resource(Frozen(frozen.clone()))
            .add_systems(bevy_app::Update, Frozen::check)
            .set_runner(
                WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx).into_runner(),
            );
        assert_eq!(app.run(), AppExit::Success);
        assert!(frozen.load(std::sync::atomic::Ordering::SeqCst));
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn a_burst_of_wakes_costs_one_update() {
        let rt = runtime();
        let (wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (_signal_tx, signal_rx) = mpsc::channel(1);
        for _ in 0..5 {
            wake_tx.send(()).unwrap();
        }

        let mut runner = WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx);
        let outcome = runner.park(Duration::from_secs(30));

        assert_eq!(outcome, ParkOutcome::Woken);
        assert!(
            runner.wake_rx.try_recv().is_err(),
            "the other four wakes should have been coalesced into this update"
        );
    }

    #[test]
    fn an_idle_daemon_wakes_only_on_the_housekeeping_floor() {
        let rt = runtime();
        let (_wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (_signal_tx, signal_rx) = mpsc::channel(1);

        let mut runner = WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx);
        let outcome = runner.park(Duration::from_millis(10));

        assert_eq!(outcome, ParkOutcome::TimedOut);
    }

    #[test]
    fn a_signal_stops_the_runner() {
        let rt = runtime();
        let (_wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (signal_tx, signal_rx) = mpsc::channel(1);
        signal_tx.try_send(()).unwrap();

        let mut runner = WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx);
        let outcome = runner.park(Duration::from_secs(30));

        assert_eq!(outcome, ParkOutcome::Shutdown);
    }

    #[test]
    fn the_server_dropping_its_wake_sender_stops_the_runner() {
        let rt = runtime();
        let (wake_tx, wake_rx) = mpsc::unbounded_channel::<()>();
        let (_signal_tx, signal_rx) = mpsc::channel(1);
        drop(wake_tx);

        let mut runner = WakeDrivenRunner::new(rt.handle().clone(), wake_rx, signal_rx);
        let outcome = runner.park(Duration::from_secs(30));

        assert_eq!(outcome, ParkOutcome::Shutdown);
    }

    #[test]
    fn the_app_exits_when_a_signal_arrives() {
        let rt = runtime();
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
