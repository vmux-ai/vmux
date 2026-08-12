//! The daemon's headless Bevy app and the runner that drives it.
//!
//! Bevy is frame-driven and a daemon has no vsync, so a fixed tick is wrong at both ends: a fast
//! one burns CPU on a box billed by the second, a slow one adds latency to every streamed frame.
//! The runner instead parks on the wake channel the PTY readers already send to, and treats its
//! timeout as a floor for housekeeping rather than as the pacing mechanism.
//!
//! This mirrors the rule the desktop follows for the same reason. `UpdateMode::Continuous` is
//! banned there because it costs 100-200% idle CPU; in a container the same mistake is a bill,
//! and it defeats the idle detection that suspend and scale-to-zero depend on.

use std::time::Duration;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::protocol::ProcessId;

/// How long the runner sleeps when nothing wakes it.
///
/// A floor for housekeeping, not a frame rate. Work arrives through the wake channel; anything
/// that needs this to be short is queued on the wrong edge.
const HOUSEKEEPING_FLOOR: Duration = Duration::from_secs(1);

/// The Tokio runtime the daemon's async work runs on. Process-wide with no per-entity identity,
/// which is what a resource is for.
#[derive(Resource, Clone)]
pub struct ServiceRuntime(pub Handle);

/// Wires the daemon's headless app. Only the runtime handle so far — session, ACP and process
/// state move onto entities in later stages.
pub struct ServiceHostPlugin {
    pub runtime: Handle,
}

impl Plugin for ServiceHostPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ServiceRuntime(self.runtime.clone()));
    }
}

/// Why the runner stopped parking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParkOutcome {
    Woken,
    TimedOut,
    /// A signal handler asked the process to stop, or every wake sender was dropped because the
    /// IPC server finished.
    Shutdown,
}

/// Block until something needs doing.
///
/// Coalesces on purpose: a burst of wakes costs one update rather than one per message, which is
/// what stops a chatty PTY from driving the schedule once per read.
pub fn park_for_wake(
    runtime: &Handle,
    wake_rx: &mut mpsc::UnboundedReceiver<ProcessId>,
    signal_rx: &mut mpsc::Receiver<()>,
    floor: Duration,
) -> ParkOutcome {
    let outcome = runtime.block_on(async {
        tokio::select! {
            wake = wake_rx.recv() => match wake {
                Some(_) => ParkOutcome::Woken,
                None => ParkOutcome::Shutdown,
            },
            _ = signal_rx.recv() => ParkOutcome::Shutdown,
            _ = tokio::time::sleep(floor) => ParkOutcome::TimedOut,
        }
    });

    if outcome == ParkOutcome::Woken {
        while wake_rx.try_recv().is_ok() {}
    }
    outcome
}

/// The `App::set_runner` body: update once per wake, then park.
pub fn wake_driven_runner(
    runtime: Handle,
    mut wake_rx: mpsc::UnboundedReceiver<ProcessId>,
    mut signal_rx: mpsc::Receiver<()>,
) -> impl FnOnce(App) -> AppExit {
    move |mut app: App| {
        loop {
            app.update();
            if let Some(exit) = app.should_exit() {
                return exit;
            }
            if park_for_wake(&runtime, &mut wake_rx, &mut signal_rx, HOUSEKEEPING_FLOOR)
                == ParkOutcome::Shutdown
            {
                return AppExit::Success;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Multi-threaded to match the daemon. On a current-thread runtime the timer is only
    /// advanced from inside `Runtime::block_on`, so the `Handle::block_on` the runner uses would
    /// park on a `sleep` that never fires.
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
        let (wake_tx, mut wake_rx) = mpsc::unbounded_channel();
        let (_signal_tx, mut signal_rx) = mpsc::channel(1);
        for _ in 0..5 {
            wake_tx.send(ProcessId::new()).unwrap();
        }

        let outcome = park_for_wake(
            rt.handle(),
            &mut wake_rx,
            &mut signal_rx,
            Duration::from_secs(30),
        );

        assert_eq!(outcome, ParkOutcome::Woken);
        assert!(
            wake_rx.try_recv().is_err(),
            "the other four wakes should have been coalesced into this update"
        );
    }

    #[test]
    fn an_idle_daemon_wakes_only_on_the_housekeeping_floor() {
        let rt = runtime();
        let (_wake_tx, mut wake_rx) = mpsc::unbounded_channel();
        let (_signal_tx, mut signal_rx) = mpsc::channel(1);

        let outcome = park_for_wake(
            rt.handle(),
            &mut wake_rx,
            &mut signal_rx,
            Duration::from_millis(10),
        );

        assert_eq!(outcome, ParkOutcome::TimedOut);
    }

    #[test]
    fn a_signal_stops_the_runner() {
        let rt = runtime();
        let (_wake_tx, mut wake_rx) = mpsc::unbounded_channel();
        let (signal_tx, mut signal_rx) = mpsc::channel(1);
        signal_tx.try_send(()).unwrap();

        let outcome = park_for_wake(
            rt.handle(),
            &mut wake_rx,
            &mut signal_rx,
            Duration::from_secs(30),
        );

        assert_eq!(outcome, ParkOutcome::Shutdown);
    }

    /// The IPC server owns the only wake senders, so its exit has to end the app too. Without
    /// this the daemon would park on a channel nobody can send to until the floor elapsed,
    /// forever.
    #[test]
    fn the_server_dropping_its_wake_sender_stops_the_runner() {
        let rt = runtime();
        let (wake_tx, mut wake_rx) = mpsc::unbounded_channel::<ProcessId>();
        let (_signal_tx, mut signal_rx) = mpsc::channel(1);
        drop(wake_tx);

        let outcome = park_for_wake(
            rt.handle(),
            &mut wake_rx,
            &mut signal_rx,
            Duration::from_secs(30),
        );

        assert_eq!(outcome, ParkOutcome::Shutdown);
    }

    #[test]
    fn the_app_exits_when_a_signal_arrives() {
        let rt = runtime();
        let (wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (signal_tx, signal_rx) = mpsc::channel(1);
        signal_tx.try_send(()).unwrap();

        let mut app = App::new();
        app.add_plugins(ServiceHostPlugin {
            runtime: rt.handle().clone(),
        })
        .set_runner(wake_driven_runner(rt.handle().clone(), wake_rx, signal_rx));

        assert_eq!(app.run(), AppExit::Success);
        drop(wake_tx);
    }
}
