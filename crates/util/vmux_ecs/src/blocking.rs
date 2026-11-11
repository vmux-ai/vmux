use std::cell::RefCell;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::task::Waker;

use bevy_ecs::prelude::Resource;

type Work = Box<dyn FnOnce() + Send + 'static>;

thread_local! {
    static CANCELLATION: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
    static CANCELLATION_GROUP: RefCell<Option<CancellationGroup>> = const { RefCell::new(None) };
}

#[derive(Resource)]
pub struct BlockingRuntime {
    short: BlockingPool,
    long: BlockingPool,
    process: BlockingPool,
}

impl Default for BlockingRuntime {
    fn default() -> Self {
        Self {
            short: BlockingPool::new("vmux-blocking-short", 4, 64),
            long: BlockingPool::new("vmux-blocking-long", 2, 16),
            process: BlockingPool::new("vmux-blocking-process", 2, 16),
        }
    }
}

impl BlockingRuntime {
    pub fn new(short: BlockingPool, long: BlockingPool, process: BlockingPool) -> Self {
        Self {
            short,
            long,
            process,
        }
    }

    pub fn try_spawn<T: Send + 'static>(
        &self,
        lane: Lane,
        wake: Option<Waker>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Result<BlockingTask<T>, SpawnError> {
        match lane {
            Lane::Short => self.short.try_spawn(wake, run),
            Lane::Long => self.long.try_spawn(wake, run),
            Lane::Process => self.process.try_spawn(wake, run),
        }
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        wake: Option<Waker>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.short.spawn(wake, run)
    }

    pub fn spawn_long<T: Send + 'static>(
        &self,
        wake: Option<Waker>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.long.spawn(wake, run)
    }

    pub fn spawn_process<T: Send + 'static>(
        &self,
        wake: Option<Waker>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Result<BlockingTask<T>, SpawnError> {
        self.process.enqueue(wake, run, false)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lane {
    #[default]
    Short,
    Long,
    Process,
}

pub struct BlockingPool {
    sender: Option<mpsc::SyncSender<Work>>,
    capacity_wake: Arc<CapacityWake>,
}

impl BlockingPool {
    pub fn new(name: &str, workers: usize, capacity: usize) -> Self {
        let capacity_wake = Arc::new(CapacityWake::default());
        if cfg!(target_family = "wasm") || workers == 0 || capacity == 0 {
            return Self {
                sender: None,
                capacity_wake,
            };
        }
        let (sender, receiver) = mpsc::sync_channel::<Work>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut running = 0;
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
            let capacity_wake = Arc::clone(&capacity_wake);
            let name = format!("{name}-{index}");
            if std::thread::Builder::new()
                .name(name)
                .spawn(move || {
                    loop {
                        let work = {
                            let Ok(receiver) = receiver.lock() else {
                                return;
                            };
                            receiver.recv()
                        };
                        let Ok(work) = work else {
                            return;
                        };
                        capacity_wake.notify();
                        work();
                    }
                })
                .is_ok()
            {
                running += 1;
            }
        }
        Self {
            sender: (running > 0).then_some(sender),
            capacity_wake,
        }
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        wake: Option<Waker>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.enqueue(wake, run, false).ok()
    }

    pub fn try_spawn<T: Send + 'static>(
        &self,
        wake: Option<Waker>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Result<BlockingTask<T>, SpawnError> {
        self.enqueue(wake, run, true)
    }

    fn enqueue<T: Send + 'static>(
        &self,
        wake: Option<Waker>,
        run: impl FnOnce() -> T + Send + 'static,
        notify_on_full: bool,
    ) -> Result<BlockingTask<T>, SpawnError> {
        let sender = self.sender.as_ref().ok_or(SpawnError::Unavailable)?;
        let retry_wake = if notify_on_full { wake.clone() } else { None };
        let state = Arc::new(Mutex::new(None));
        let output = Arc::clone(&state);
        let canceled = Arc::new(AtomicBool::new(false));
        let worker_canceled = Arc::clone(&canceled);
        let work = Box::new(move || {
            let wake = CompletionWake(wake);
            if worker_canceled.load(Ordering::Acquire) {
                return;
            }
            let previous = CANCELLATION
                .with(|current| current.borrow_mut().replace(Arc::clone(&worker_canceled)));
            let scope = CancellationScope { previous };
            let result = catch_unwind(AssertUnwindSafe(run));
            drop(scope);
            if let Ok(mut output) = output.lock()
                && !worker_canceled.load(Ordering::Acquire)
            {
                *output = Some(result);
            }
            drop(wake);
        });
        self.capacity_wake.send(sender, work, retry_wake)?;
        Ok(BlockingTask { state, canceled })
    }
}

#[derive(Default)]
struct CapacityWake {
    waiters: Mutex<Vec<Waker>>,
}

impl CapacityWake {
    fn send(
        &self,
        sender: &mpsc::SyncSender<Work>,
        work: Work,
        mut wake: Option<Waker>,
    ) -> Result<(), SpawnError> {
        let result = {
            let mut waiters = self.waiters.lock().map_err(|_| SpawnError::Unavailable)?;
            let result = sender.try_send(work);
            if matches!(&result, Err(mpsc::TrySendError::Full(_)))
                && let Some(waker) = &wake
            {
                let mut registered = false;
                for current in &*waiters {
                    if current.will_wake(waker) {
                        registered = true;
                        break;
                    }
                }
                if !registered {
                    waiters.push(wake.take().unwrap());
                }
            }
            result
        };
        match result {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => Err(SpawnError::Full),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(SpawnError::Unavailable),
        }
    }

    fn notify(&self) {
        let waiters = {
            let Ok(mut waiters) = self.waiters.lock() else {
                return;
            };
            std::mem::take(&mut *waiters)
        };
        for wake in waiters {
            drop(CompletionWake(Some(wake)));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    Full,
    Unavailable,
}

impl fmt::Display for SpawnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Full => "blocking worker queue is full",
            Self::Unavailable => "blocking worker pool is unavailable",
        })
    }
}

impl std::error::Error for SpawnError {}

struct CompletionWake(Option<Waker>);

impl Drop for CompletionWake {
    fn drop(&mut self) {
        if let Some(wake) = &self.0 {
            let _ = catch_unwind(AssertUnwindSafe(|| wake.wake_by_ref()));
        }
    }
}

pub struct BlockingCancellation;

impl BlockingCancellation {
    pub fn canceled() -> bool {
        let canceled = CANCELLATION.with(|current| {
            current
                .borrow()
                .as_ref()
                .is_some_and(|canceled| canceled.load(Ordering::Acquire))
        });
        if !canceled {
            return false;
        }
        CANCELLATION_GROUP.with(|current| {
            current
                .borrow()
                .as_ref()
                .is_none_or(CancellationGroup::all_canceled)
        })
    }

    pub fn without<T>(run: impl FnOnce() -> T) -> T {
        let previous = CANCELLATION.with(|current| current.borrow_mut().take());
        let scope = CancellationScope { previous };
        let output = run();
        drop(scope);
        output
    }
}

#[derive(Clone, Default)]
pub struct CancellationGroup(Arc<Mutex<Vec<Weak<AtomicBool>>>>);

impl CancellationGroup {
    pub fn join(&self) -> CancellationParticipant {
        let canceled = CANCELLATION.with(|current| current.borrow().clone());
        let member = canceled.as_ref().map(Arc::downgrade);
        if let Some(member) = &member
            && let Ok(mut members) = self.0.lock()
        {
            members.push(member.clone());
        }
        CancellationParticipant {
            group: self.clone(),
            member,
        }
    }

    fn all_canceled(&self) -> bool {
        let Ok(mut members) = self.0.lock() else {
            return true;
        };
        members.retain(|member| member.strong_count() > 0);
        members.iter().all(|member| {
            member
                .upgrade()
                .is_none_or(|canceled| canceled.load(Ordering::Acquire))
        })
    }
}

pub struct CancellationParticipant {
    group: CancellationGroup,
    member: Option<Weak<AtomicBool>>,
}

impl CancellationParticipant {
    pub fn run<T>(&self, run: impl FnOnce() -> T) -> T {
        let previous =
            CANCELLATION_GROUP.with(|current| current.borrow_mut().replace(self.group.clone()));
        let scope = CancellationGroupScope { previous };
        let output = run();
        drop(scope);
        output
    }
}

impl Drop for CancellationParticipant {
    fn drop(&mut self) {
        let Some(member) = &self.member else {
            return;
        };
        if let Ok(mut members) = self.group.0.lock() {
            members.retain(|candidate| !Weak::ptr_eq(candidate, member));
        }
    }
}

struct CancellationGroupScope {
    previous: Option<CancellationGroup>,
}

impl Drop for CancellationGroupScope {
    fn drop(&mut self) {
        CANCELLATION_GROUP.with(|current| {
            *current.borrow_mut() = self.previous.take();
        });
    }
}

struct CancellationScope {
    previous: Option<Arc<AtomicBool>>,
}

impl Drop for CancellationScope {
    fn drop(&mut self) {
        CANCELLATION.with(|current| {
            *current.borrow_mut() = self.previous.take();
        });
    }
}

pub struct BlockingTask<T> {
    state: Arc<Mutex<Option<std::thread::Result<T>>>>,
    canceled: Arc<AtomicBool>,
}

impl<T> BlockingTask<T> {
    pub fn poll(&mut self) -> Option<std::thread::Result<T>> {
        self.state.lock().ok()?.take()
    }

    pub fn cancel(&mut self) {
        self.canceled.store(true, Ordering::Release);
        if let Ok(mut state) = self.state.lock() {
            state.take();
        }
    }
}

impl<T> Drop for BlockingTask<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    struct PublicationWake {
        task: Mutex<Option<BlockingTask<u32>>>,
        observed: mpsc::SyncSender<Option<u32>>,
    }

    impl std::task::Wake for PublicationWake {
        fn wake(self: Arc<Self>) {
            let result = self
                .task
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .poll()
                .map(Result::unwrap);
            self.observed.send(result).unwrap();
        }
    }

    #[test]
    fn completion_publishes_the_result_before_invoking_the_host_waker() {
        let pool = BlockingPool::new("test-publication", 1, 1);
        let (observed_tx, observed_rx) = mpsc::sync_channel(1);
        let wake = Arc::new(PublicationWake {
            task: Mutex::new(None),
            observed: observed_tx,
        });
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let task = pool
            .try_spawn(Some(Arc::clone(&wake).into()), move || {
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                17
            })
            .unwrap();
        *wake.task.lock().unwrap() = Some(task);
        release_tx.send(()).unwrap();
        assert_eq!(
            observed_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            Some(17)
        );
    }

    #[test]
    fn a_full_queue_rejects_work_and_canceled_queued_work_never_runs() {
        let pool = BlockingPool::new("test-queue", 1, 1);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let mut running = pool
            .spawn(None, move || {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let (executed_tx, executed_rx) = mpsc::sync_channel(1);
        let canceled = pool
            .spawn(None, move || executed_tx.send(()).unwrap())
            .unwrap();
        assert!(matches!(pool.try_spawn(None, || ()), Err(SpawnError::Full)));
        drop(canceled);
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while running.poll().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let mut following = loop {
            if let Some(task) = pool.spawn(None, || 7) {
                break task;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        loop {
            if let Some(result) = following.poll() {
                assert_eq!(result.unwrap(), 7);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(matches!(
            executed_rx.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn a_panicking_job_returns_failure_and_the_worker_accepts_the_next_job() {
        let pool = BlockingPool::new("test-panic", 1, 1);
        let mut failed = pool.spawn(None, || panic!("worker failure")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = failed.poll() {
                assert!(result.is_err());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let mut following = pool.spawn(None, || 9).unwrap();
        loop {
            if let Some(result) = following.poll() {
                assert_eq!(result.unwrap(), 9);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    struct PanicWake;

    impl std::task::Wake for PanicWake {
        fn wake(self: Arc<Self>) {
            panic!("host wake failure");
        }
    }

    #[test]
    fn a_panicking_host_waker_does_not_discard_queued_work() {
        let pool = BlockingPool::new("test-wake-panic", 1, 1);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let mut first = pool
            .try_spawn(Some(Arc::new(PanicWake).into()), move || {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                7
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut following = pool.try_spawn(None, || 9).unwrap();
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = following.poll() {
                assert_eq!(result.unwrap(), 9);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(first.poll().unwrap().unwrap(), 7);
    }

    #[test]
    fn unusable_pools_report_unavailable_instead_of_queue_backpressure() {
        for (workers, capacity) in [(0, 1), (1, 0), (0, 0)] {
            let pool = BlockingPool::new("test-unavailable", workers, capacity);
            assert!(matches!(
                pool.try_spawn(None, || panic!("unavailable work must not run")),
                Err(SpawnError::Unavailable)
            ));
        }
    }

    struct CapacityNotification(mpsc::Sender<()>);

    impl std::task::Wake for CapacityNotification {
        fn wake(self: Arc<Self>) {
            self.0.send(()).unwrap();
        }
    }

    #[test]
    fn capacity_notifications_wake_sdk_retries_without_changing_native_completion_wakes() {
        for retry_wake in [true, false] {
            let pool = BlockingPool::new("test-capacity-wake", 1, 1);
            let (started_tx, started_rx) = mpsc::sync_channel(1);
            let (release_tx, release_rx) = mpsc::sync_channel(1);
            let _running = pool
                .spawn(None, move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                })
                .unwrap();
            started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            let (queued_started_tx, queued_started_rx) = mpsc::sync_channel(1);
            let (queued_release_tx, queued_release_rx) = mpsc::sync_channel(1);
            let _queued = pool
                .spawn(None, move || {
                    queued_started_tx.send(()).unwrap();
                    queued_release_rx
                        .recv_timeout(Duration::from_secs(2))
                        .unwrap();
                })
                .unwrap();
            let (wake_tx, wake_rx) = mpsc::channel();
            let wake = Arc::new(CapacityNotification(wake_tx));
            for _ in 0..4 {
                if retry_wake {
                    assert!(matches!(
                        pool.try_spawn(Some(Arc::clone(&wake).into()), || 11),
                        Err(SpawnError::Full)
                    ));
                } else {
                    assert!(pool.spawn(Some(Arc::clone(&wake).into()), || 11).is_none());
                }
            }
            release_tx.send(()).unwrap();
            queued_started_rx
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            if retry_wake {
                wake_rx.recv_timeout(Duration::from_secs(1)).unwrap();
            }
            assert!(matches!(wake_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
            let mut accepted = pool.try_spawn(None, || 9).unwrap();
            queued_release_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if let Some(result) = accepted.poll() {
                    assert_eq!(result.unwrap(), 9);
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
    }

    #[test]
    fn cancel_discards_an_unpolled_result_after_the_worker_finishes() {
        let pool = BlockingPool::new("test-finished-cancel", 1, 2);
        let mut canceled = pool.spawn(None, || 7).unwrap();
        let mut following = pool.spawn(None, || ()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while following.poll().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        canceled.cancel();
        assert!(canceled.poll().is_none());
    }
}
