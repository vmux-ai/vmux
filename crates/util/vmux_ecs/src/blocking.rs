use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak, mpsc};

use bevy::prelude::Resource;
use bevy::winit::{EventLoopProxy, WinitUserEvent};

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
    pub fn spawn<T: Send + 'static>(
        &self,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.short.spawn(wake, run)
    }

    pub fn spawn_long<T: Send + 'static>(
        &self,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.long.spawn(wake, run)
    }

    pub fn spawn_process<T: Send + 'static>(
        &self,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.process.spawn(wake, run)
    }
}

pub struct BlockingPool {
    sender: Option<mpsc::SyncSender<Work>>,
}

impl BlockingPool {
    pub fn new(name: &str, workers: usize, capacity: usize) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Work>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut running = 0;
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
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
        }
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        let sender = self.sender.as_ref()?;
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
            if !worker_canceled.load(Ordering::Acquire)
                && let Ok(mut output) = output.lock()
            {
                *output = Some(result);
            }
            drop(wake);
        });
        if sender.try_send(work).is_err() {
            return None;
        }
        Some(BlockingTask { state, canceled })
    }
}

struct CompletionWake(Option<EventLoopProxy<WinitUserEvent>>);

impl Drop for CompletionWake {
    fn drop(&mut self) {
        if let Some(wake) = &self.0 {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
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
        assert!(pool.spawn(None, || ()).is_none());
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
}
