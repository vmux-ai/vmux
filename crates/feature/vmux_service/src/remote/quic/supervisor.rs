use tokio::sync::watch;

use super::super::server::RemoteState;
use super::dialer::RegisteredDevice;

pub(crate) struct Supervisor<D> {
    dialer: D,
    exposed: watch::Receiver<bool>,
    running: Option<tokio::task::JoinHandle<()>>,
}

impl Supervisor<RelayDialer> {
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

    async fn stop(&mut self) {
        let Some(task) = self.running.take() else {
            return;
        };
        task.abort();
        let _ = task.await;
        tracing::info!("remote quic: the relay dialer stopped");
    }
}

pub(crate) trait Dial: Send + 'static {
    fn dial(&mut self) -> tokio::task::JoinHandle<()>;
}

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
