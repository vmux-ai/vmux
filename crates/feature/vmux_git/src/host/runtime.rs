use bevy::prelude::Resource;
use bevy::winit::{EventLoopProxy, WinitUserEvent};
use vmux_ecs::blocking::{BlockingCancellation, BlockingPool};

pub use vmux_ecs::blocking::BlockingTask as GitTask;
pub(super) use vmux_ecs::blocking::{
    CancellationGroup as GitCancellationGroup,
    CancellationParticipant as GitCancellationParticipant,
};

const INTERACTIVE_WORKER_COUNT: usize = 4;
const INTERACTIVE_QUEUE_CAPACITY: usize = 64;
const BACKGROUND_WORKER_COUNT: usize = 2;
const BACKGROUND_QUEUE_CAPACITY: usize = 256;
const OPERATION_WORKER_COUNT: usize = 1;
const OPERATION_QUEUE_CAPACITY: usize = 8;

#[derive(Resource)]
pub struct GitRuntime {
    interactive: BlockingPool,
    background: BlockingPool,
    operations: BlockingPool,
}

impl Default for GitRuntime {
    fn default() -> Self {
        Self {
            interactive: BlockingPool::new(
                "vmux-git",
                INTERACTIVE_WORKER_COUNT,
                INTERACTIVE_QUEUE_CAPACITY,
            ),
            background: BlockingPool::new(
                "vmux-git-background",
                BACKGROUND_WORKER_COUNT,
                BACKGROUND_QUEUE_CAPACITY,
            ),
            operations: BlockingPool::new(
                "vmux-git-operation",
                OPERATION_WORKER_COUNT,
                OPERATION_QUEUE_CAPACITY,
            ),
        }
    }
}

impl GitRuntime {
    pub fn spawn<T: Send + 'static>(
        &self,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<GitTask<T>> {
        self.interactive
            .spawn(vmux_ecs::wake::Wake::optional_waker(wake), run)
    }

    pub fn spawn_operation<T: Send + 'static>(
        &self,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<GitTask<T>> {
        self.operations
            .spawn(vmux_ecs::wake::Wake::optional_waker(wake), run)
    }

    pub fn spawn_background<T: Send + 'static>(
        &self,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<GitTask<T>> {
        self.background
            .spawn(vmux_ecs::wake::Wake::optional_waker(wake), run)
    }

    pub(super) fn canceled() -> bool {
        BlockingCancellation::canceled()
    }

    pub(super) fn without_cancellation<T>(run: impl FnOnce() -> T) -> T {
        BlockingCancellation::without(run)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn runtime_bounds_concurrent_work() {
        let runtime = GitRuntime::default();
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            let task = loop {
                let active = Arc::clone(&active);
                let maximum = Arc::clone(&maximum);
                if let Some(task) = runtime.spawn(None, move || {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(current, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(5));
                    active.fetch_sub(1, Ordering::SeqCst);
                }) {
                    break task;
                }
                std::thread::yield_now();
            };
            tasks.push(Some(task));
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
        assert_eq!(maximum.load(Ordering::SeqCst), INTERACTIVE_WORKER_COUNT);
    }

    #[test]
    fn dropping_a_task_terminates_its_git_process() {
        let runtime = GitRuntime::default();
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("started");
        let alias = format!("alias.vmux-wait=!touch {}; sleep 10", marker.display());
        let root = directory.path().to_path_buf();
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let task = runtime
            .spawn(None, move || {
                let started = Instant::now();
                let result =
                    super::super::repository::GitCommand::run(&root, &["-c", &alias, "vmux-wait"]);
                done_tx.send((started.elapsed(), result)).unwrap();
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(task);
        let (elapsed, result) = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(elapsed < Duration::from_secs(2));
        assert_eq!(result.unwrap_err().0, "git command canceled");
    }

    #[test]
    fn shared_cancellation_waits_for_the_last_participant() {
        let runtime = GitRuntime::default();
        let group = GitCancellationGroup::default();
        let (started_tx, started_rx) = mpsc::sync_channel(2);
        let (stopped_tx, stopped_rx) = mpsc::sync_channel(0);
        let leader_group = group.clone();
        let leader_started = started_tx.clone();
        let leader_stopped = stopped_tx.clone();
        let leader = runtime
            .spawn(None, move || {
                let leader = leader_group.join();
                leader.run(|| {
                    leader_started.send(()).unwrap();
                    while !GitRuntime::canceled() {
                        std::thread::yield_now();
                    }
                    leader_stopped.send(()).unwrap();
                });
            })
            .unwrap();
        let follower_group = group.clone();
        let follower = runtime
            .spawn(None, move || {
                let follower = follower_group.join();
                follower.run(|| {
                    started_tx.send(()).unwrap();
                    while !GitRuntime::canceled() {
                        std::thread::yield_now();
                    }
                    stopped_tx.send(()).unwrap();
                });
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        drop(leader);
        assert!(stopped_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(follower);
        stopped_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        stopped_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn operations_do_not_block_interactive_work() {
        let runtime = GitRuntime::default();
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let (started_tx, started_rx) = mpsc::sync_channel(0);
        let operation = runtime
            .spawn_operation(None, move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let mut interactive = runtime.spawn(None, || 7).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let value = loop {
            if let Some(value) = interactive.poll() {
                break value.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert_eq!(value, 7);
        release_tx.send(()).unwrap();
        drop(operation);
    }

    #[test]
    fn background_work_does_not_block_interactive_work() {
        let runtime = GitRuntime::default();
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let (started_tx, started_rx) = mpsc::sync_channel(0);
        let background = runtime
            .spawn_background(None, move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let mut interactive = runtime.spawn(None, || 7).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let value = loop {
            if let Some(value) = interactive.poll() {
                break value.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert_eq!(value, 7);
        release_tx.send(()).unwrap();
        drop(background);
    }
}
