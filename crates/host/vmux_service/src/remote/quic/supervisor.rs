//! The Remote switch as a lifecycle, not only an access check.
//!
//! Gating admission was never the whole job. A desktop that dials the relay whatever the switch
//! says still registers, still holds an allocated port, and still retries forever — it is merely
//! advertised as a desktop that refuses everyone, and a registration that fails asks the user to
//! attend to a feature they never turned on.
//!
//! So the switch owns the dialer's lifetime instead. While Remote is off there is no dial, no
//! registration and nothing to fail. [`super::admit`] stays exactly where it was, because a phone
//! that authenticated before the switch moved is already connected and has to be dropped by the
//! connection it is on rather than by never having been dialled for.

use tokio::sync::watch;

use super::super::server::RemoteState;
use super::dialer::RegisteredDevice;

/// Keeps the relay dialer running for exactly as long as Remote is switched on.
///
/// The switch is polled rather than pushed, so the daemon converges on it whichever order the two
/// processes start in: the daemon reads it at boot, the desktop may write it moments later, and
/// the next tick reconciles the difference either way.
pub(crate) struct Supervisor<D> {
    dialer: D,
    exposed: watch::Receiver<bool>,
    running: Option<tokio::task::JoinHandle<()>>,
}

impl Supervisor<RelayDialer> {
    /// Watch the switch and keep the relay dialer in step with it, for as long as the daemon runs.
    pub(crate) fn spawn(state: RemoteState) -> tokio::task::JoinHandle<()> {
        let exposed = super::spawn_liveness_watch();
        let dialer = RelayDialer {
            state,
            exposed: exposed.clone(),
        };
        tokio::spawn(Supervisor::new(dialer, exposed).run())
    }
}

impl<D: Dial> Supervisor<D> {
    fn new(dialer: D, exposed: watch::Receiver<bool>) -> Self {
        Self {
            dialer,
            exposed,
            running: None,
        }
    }

    /// Follow the switch until it can move no more.
    ///
    /// A port recorded by a previous process is cleared once here rather than when the dialer
    /// starts: a daemon that was killed never ran the guard that clears it, and a reader asking
    /// the moment Remote is switched back on would otherwise be handed the dead one.
    async fn run(mut self) {
        RegisteredDevice::release_stale();
        loop {
            self.reconcile().await;
            if self.exposed.changed().await.is_err() {
                break;
            }
        }
        self.stop().await;
    }

    /// Match the running dialer to the switch.
    async fn reconcile(&mut self) {
        if *self.exposed.borrow() {
            self.start();
        } else {
            self.stop().await;
        }
    }

    fn start(&mut self) {
        if self.running.is_some() {
            return;
        }
        tracing::info!("remote quic: dialing the relay");
        self.running = Some(self.dialer.dial());
    }

    /// Take the dialer down, and wait until it is actually gone.
    ///
    /// Awaiting the aborted task is what makes the teardown observable rather than merely
    /// requested: the allocated port is released by a guard the dialer holds, and returning before
    /// that guard has dropped would leave the relay's answer on disk for a session that no longer
    /// exists.
    async fn stop(&mut self) {
        let Some(task) = self.running.take() else {
            return;
        };
        task.abort();
        let _ = task.await;
        tracing::info!("remote quic: the relay dialer stopped");
    }
}

/// What the supervisor brings up while Remote is on.
///
/// A trait rather than a direct call so the lifecycle can be exercised without a relay to dial:
/// the question these tests answer — whether a dialer is running — cannot be asked of a task that
/// insists on reaching the network first.
pub(crate) trait Dial: Send + 'static {
    fn dial(&mut self) -> tokio::task::JoinHandle<()>;
}

/// Dials the real relay.
pub(crate) struct RelayDialer {
    state: RemoteState,
    exposed: watch::Receiver<bool>,
}

impl Dial for RelayDialer {
    fn dial(&mut self) -> tokio::task::JoinHandle<()> {
        super::dialer::spawn(self.state.clone(), self.exposed.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A dialer that reaches nothing and counts instead.
    ///
    /// `live` is decremented by a guard the spawned task holds, mirroring the allocated port: it
    /// answers whether the task was really torn down, not merely whether abort was called.
    #[derive(Clone, Default)]
    struct Counted {
        dials: Arc<AtomicUsize>,
        live: Arc<AtomicUsize>,
    }

    impl Counted {
        fn dials(&self) -> usize {
            self.dials.load(Ordering::SeqCst)
        }

        fn live(&self) -> usize {
            self.live.load(Ordering::SeqCst)
        }
    }

    impl Dial for Counted {
        fn dial(&mut self) -> tokio::task::JoinHandle<()> {
            self.dials.fetch_add(1, Ordering::SeqCst);
            let live = Live(self.live.clone());
            live.0.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let _live = live;
                std::future::pending::<()>().await;
            })
        }
    }

    struct Live(Arc<AtomicUsize>);

    impl Drop for Live {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// The bug this guards: the dialer used to start with the daemon and only refuse phones once
    /// they arrived, so a desktop with Remote off still registered and still reported failures.
    #[tokio::test]
    async fn a_switch_that_is_off_dials_nothing() {
        let (_exposed, receiver) = watch::channel(false);
        let dialer = Counted::default();
        let mut supervisor = Supervisor::new(dialer.clone(), receiver);

        supervisor.reconcile().await;

        assert_eq!(dialer.dials(), 0, "a disabled Remote dialled the relay");
    }

    #[tokio::test]
    async fn the_switch_starts_and_stops_the_dialer() {
        let (exposed, receiver) = watch::channel(true);
        let dialer = Counted::default();
        let mut supervisor = Supervisor::new(dialer.clone(), receiver);

        supervisor.reconcile().await;
        assert_eq!(dialer.dials(), 1);
        assert_eq!(dialer.live(), 1);

        exposed.send(false).expect("switch off");
        supervisor.reconcile().await;
        assert_eq!(dialer.live(), 0, "the dialer outlived the switch");

        exposed.send(true).expect("switch on");
        supervisor.reconcile().await;
        assert_eq!(dialer.dials(), 2, "switching back on did not dial again");
        assert_eq!(dialer.live(), 1);
    }

    /// A switch that stays on must not stack dialers on top of each other; every tick reconciles,
    /// and only a change is a reason to start one.
    #[tokio::test]
    async fn a_switch_left_on_keeps_one_dialer() {
        let (_exposed, receiver) = watch::channel(true);
        let dialer = Counted::default();
        let mut supervisor = Supervisor::new(dialer.clone(), receiver);

        for _ in 0..3 {
            supervisor.reconcile().await;
        }

        assert_eq!(dialer.dials(), 1);
        assert_eq!(dialer.live(), 1);
    }
}
