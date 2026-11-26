use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use bevy_ecs::prelude::Component;
use tokio::runtime::Handle;
use tokio::task::{JoinError, JoinHandle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokioTaskOutcome {
    Completed,
    Canceled,
    Panicked,
}

impl fmt::Display for TokioTaskOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Completed => formatter.write_str("tokio task completed"),
            Self::Canceled => formatter.write_str("tokio task canceled"),
            Self::Panicked => formatter.write_str("tokio task panicked"),
        }
    }
}

#[derive(Component)]
pub struct TokioTask {
    handle: Option<JoinHandle<()>>,
}

impl TokioTask {
    pub fn new(handle: JoinHandle<()>) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    pub fn spawn<F>(runtime: &Handle, future: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Self::new(runtime.spawn(future))
    }

    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn abort(&self) {
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }

    pub fn take_outcome(&mut self) -> Option<TokioTaskOutcome> {
        if !self.handle.as_ref()?.is_finished() {
            return None;
        }
        self.poll_outcome(Waker::noop())
    }

    pub fn poll_outcome(&mut self, waker: &Waker) -> Option<TokioTaskOutcome> {
        let handle = self.handle.as_mut()?;
        let mut context = Context::from_waker(waker);
        let Poll::Ready(result) = Pin::new(handle).poll(&mut context) else {
            return None;
        };
        self.handle.take();
        Some(Self::classify(result))
    }

    pub async fn join(&mut self) -> Option<TokioTaskOutcome> {
        let result = self.handle.as_mut()?.await;
        self.handle.take();
        Some(Self::classify(result))
    }

    fn classify(result: Result<(), JoinError>) -> TokioTaskOutcome {
        match result {
            Ok(()) => TokioTaskOutcome::Completed,
            Err(error) if error.is_panic() => TokioTaskOutcome::Panicked,
            Err(_) => TokioTaskOutcome::Canceled,
        }
    }
}

impl From<JoinHandle<()>> for TokioTask {
    fn from(handle: JoinHandle<()>) -> Self {
        Self::new(handle)
    }
}

impl Drop for TokioTask {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::task::Wake;
    use std::time::{Duration, Instant};

    use bevy_ecs::world::World;
    use tokio::runtime::Builder;

    use super::*;

    struct DropNotice(mpsc::SyncSender<()>);

    impl Drop for DropNotice {
        fn drop(&mut self) {
            let _ = self.0.try_send(());
        }
    }

    fn wait_outcome(task: &mut TokioTask) -> TokioTaskOutcome {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(outcome) = task.take_outcome() {
                return outcome;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn despawning_an_entity_aborts_its_tokio_task() {
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_time()
            .build()
            .unwrap();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (dropped_tx, dropped_rx) = mpsc::sync_channel(1);
        let mut world = World::new();
        let entity = world
            .spawn(TokioTask::spawn(runtime.handle(), async move {
                let _notice = DropNotice(dropped_tx);
                started_tx.send(()).unwrap();
                std::future::pending::<()>().await;
            }))
            .id();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        world.entity_mut(entity).despawn();
        dropped_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn take_outcome_distinguishes_completion_cancellation_and_panic() {
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_time()
            .build()
            .unwrap();
        let mut completed = TokioTask::spawn(runtime.handle(), async {});
        assert_eq!(wait_outcome(&mut completed), TokioTaskOutcome::Completed);
        assert!(completed.is_finished());
        assert_eq!(completed.take_outcome(), None);

        let mut panicked = TokioTask::spawn(runtime.handle(), async {
            panic!("tokio worker panic");
        });
        assert_eq!(wait_outcome(&mut panicked), TokioTaskOutcome::Panicked);

        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let mut canceled = TokioTask::spawn(runtime.handle(), async move {
            started_tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        canceled.abort();
        assert_eq!(wait_outcome(&mut canceled), TokioTaskOutcome::Canceled);
    }

    struct WakeNotice(mpsc::SyncSender<()>);

    impl Wake for WakeNotice {
        fn wake(self: Arc<Self>) {
            let _ = self.0.try_send(());
        }

        fn wake_by_ref(self: &Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    #[test]
    fn poll_outcome_registers_waker_and_reports_completion() {
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_time()
            .build()
            .unwrap();
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (wake_tx, wake_rx) = mpsc::sync_channel(1);
        let waker = Waker::from(Arc::new(WakeNotice(wake_tx)));
        let mut task = TokioTask::new(runtime.spawn_blocking(move || {
            release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }));
        assert_eq!(task.poll_outcome(&waker), None);
        release_tx.send(()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(task.poll_outcome(&waker), Some(TokioTaskOutcome::Completed));
        assert_eq!(task.poll_outcome(&waker), None);
    }

    #[test]
    fn join_awaits_task_completion_and_cancellation_once() {
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_time()
            .build()
            .unwrap();
        let mut completed = TokioTask::spawn(runtime.handle(), async {});
        assert_eq!(
            runtime.block_on(completed.join()),
            Some(TokioTaskOutcome::Completed)
        );
        assert_eq!(runtime.block_on(completed.join()), None);

        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (dropped_tx, dropped_rx) = mpsc::sync_channel(1);
        let mut canceled = TokioTask::spawn(runtime.handle(), async move {
            let _notice = DropNotice(dropped_tx);
            started_tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        canceled.abort();
        assert_eq!(
            runtime.block_on(canceled.join()),
            Some(TokioTaskOutcome::Canceled)
        );
        dropped_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(runtime.block_on(canceled.join()), None);
    }
}
