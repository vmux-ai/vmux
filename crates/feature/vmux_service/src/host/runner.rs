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
