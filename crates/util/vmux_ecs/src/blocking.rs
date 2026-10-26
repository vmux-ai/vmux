use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use bevy::prelude::Resource;
use bevy::winit::WinitUserEvent;

const SHORT_WORKER_COUNT: usize = 4;
const SHORT_QUEUE_CAPACITY: usize = 64;
const LONG_WORKER_COUNT: usize = 2;
const LONG_QUEUE_CAPACITY: usize = 16;
const PROCESS_WORKER_COUNT: usize = 2;
const PROCESS_QUEUE_CAPACITY: usize = 16;

type Work = Box<dyn FnOnce() + Send + 'static>;

#[derive(Resource)]
pub struct BlockingRuntime {
    short: BlockingLane,
    long: BlockingLane,
    process: BlockingLane,
}

impl Default for BlockingRuntime {
    fn default() -> Self {
        Self {
            short: BlockingLane::new(
                "vmux-blocking-short",
                SHORT_WORKER_COUNT,
                SHORT_QUEUE_CAPACITY,
            ),
            long: BlockingLane::new("vmux-blocking-long", LONG_WORKER_COUNT, LONG_QUEUE_CAPACITY),
            process: BlockingLane::new(
                "vmux-blocking-process",
                PROCESS_WORKER_COUNT,
                PROCESS_QUEUE_CAPACITY,
            ),
        }
    }
}

impl BlockingRuntime {
    pub fn spawn<T: Send + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.short.spawn(wake, run)
    }

    pub fn spawn_long<T: Send + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.long.spawn(wake, run)
    }

    pub fn spawn_long_cancelable<T: Send + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce(BlockingCancellation) -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.long.spawn_cancelable(wake, run)
    }

    pub fn spawn_process_cancelable<T: Send + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce(BlockingCancellation) -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.process.spawn_cancelable(wake, run)
    }
}

#[derive(Clone)]
pub struct BlockingCancellation(Arc<AtomicBool>);

impl BlockingCancellation {
    pub fn is_canceled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

struct BlockingLane {
    sender: Option<mpsc::SyncSender<Work>>,
}

impl BlockingLane {
    fn new(name: &str, workers: usize, capacity: usize) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Work>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut running = 0;
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
            let thread_name = format!("{name}-{index}");
            let spawned = std::thread::Builder::new()
                .name(thread_name)
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
                });
            if spawned.is_ok() {
                running += 1;
            }
        }
        Self {
            sender: (running > 0).then_some(sender),
        }
    }

    fn spawn<T: Send + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        self.spawn_cancelable(wake, move |_| run())
    }

    fn spawn_cancelable<T: Send + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce(BlockingCancellation) -> T + Send + 'static,
    ) -> Option<BlockingTask<T>> {
        let sender = self.sender.as_ref()?;
        let state = Arc::new(Mutex::new(None));
        let output = Arc::clone(&state);
        let canceled = Arc::new(AtomicBool::new(false));
        let worker_canceled = Arc::clone(&canceled);
        let cancellation = BlockingCancellation(Arc::clone(&canceled));
        let work = Box::new(move || {
            if worker_canceled.load(Ordering::Acquire) {
                if let Some(wake) = wake {
                    let _ = wake.send_event(WinitUserEvent::WakeUp);
                }
                return;
            }
            let result = catch_unwind(AssertUnwindSafe(|| run(cancellation)));
            if !worker_canceled.load(Ordering::Acquire)
                && let Ok(mut output) = output.lock()
            {
                *output = Some(result);
            }
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
        });
        if sender.try_send(work).is_err() {
            return None;
        }
        Some(BlockingTask { state, canceled })
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
}

impl<T> Drop for BlockingTask<T> {
    fn drop(&mut self) {
        self.canceled.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn runtime_bounds_each_work_lane_independently() {
        let runtime = BlockingRuntime::default();
        let short = SelfTestLane::new();
        let long = SelfTestLane::new();
        let process = SelfTestLane::new();
        let mut tasks = Vec::new();
        for _ in 0..16 {
            tasks.push(Some(short.spawn(&runtime, false)));
            tasks.push(Some(long.spawn(&runtime, true)));
            tasks.push(Some(process.spawn_process(&runtime)));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while tasks.iter().any(Option::is_some) {
            for task in &mut tasks {
                if task.as_mut().is_some_and(|task| task.poll().is_some()) {
                    *task = None;
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(short.maximum.load(Ordering::SeqCst), SHORT_WORKER_COUNT);
        assert_eq!(long.maximum.load(Ordering::SeqCst), LONG_WORKER_COUNT);
        assert_eq!(process.maximum.load(Ordering::SeqCst), PROCESS_WORKER_COUNT);
    }

    struct SelfTestLane {
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
    }

    impl SelfTestLane {
        fn new() -> Self {
            Self {
                active: Arc::new(AtomicUsize::new(0)),
                maximum: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn spawn(&self, runtime: &BlockingRuntime, long: bool) -> BlockingTask<()> {
            let active = Arc::clone(&self.active);
            let maximum = Arc::clone(&self.maximum);
            let run = move || {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                active.fetch_sub(1, Ordering::SeqCst);
            };
            if long {
                runtime.spawn_long(None, run).unwrap()
            } else {
                runtime.spawn(None, run).unwrap()
            }
        }

        fn spawn_process(&self, runtime: &BlockingRuntime) -> BlockingTask<()> {
            let active = Arc::clone(&self.active);
            let maximum = Arc::clone(&self.maximum);
            runtime
                .spawn_process_cancelable(None, move |_| {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(current, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(5));
                    active.fetch_sub(1, Ordering::SeqCst);
                })
                .unwrap()
        }
    }
}
