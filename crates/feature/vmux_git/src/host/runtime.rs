use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak, mpsc};

use bevy::prelude::Resource;
use bevy::winit::WinitUserEvent;

const INTERACTIVE_WORKER_COUNT: usize = 4;
const INTERACTIVE_QUEUE_CAPACITY: usize = 64;
const BACKGROUND_WORKER_COUNT: usize = 2;
const BACKGROUND_QUEUE_CAPACITY: usize = 256;
const OPERATION_WORKER_COUNT: usize = 1;
const OPERATION_QUEUE_CAPACITY: usize = 8;

type Work = Box<dyn FnOnce() + Send + 'static>;

thread_local! {
    static CANCELLATION: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
    static CANCELLATION_GROUP: RefCell<Option<GitCancellationGroup>> = const { RefCell::new(None) };
}

#[derive(Resource)]
pub struct GitRuntime {
    interactive: mpsc::SyncSender<Work>,
    background: mpsc::SyncSender<Work>,
    operations: mpsc::SyncSender<Work>,
}

impl Default for GitRuntime {
    fn default() -> Self {
        Self {
            interactive: Self::workers(
                "vmux-git",
                INTERACTIVE_WORKER_COUNT,
                INTERACTIVE_QUEUE_CAPACITY,
            ),
            background: Self::workers(
                "vmux-git-background",
                BACKGROUND_WORKER_COUNT,
                BACKGROUND_QUEUE_CAPACITY,
            ),
            operations: Self::workers(
                "vmux-git-operation",
                OPERATION_WORKER_COUNT,
                OPERATION_QUEUE_CAPACITY,
            ),
        }
    }
}

impl GitRuntime {
    fn workers(name: &str, count: usize, capacity: usize) -> mpsc::SyncSender<Work> {
        let (sender, receiver) = mpsc::sync_channel::<Work>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..count {
            let receiver = Arc::clone(&receiver);
            let name = format!("{name}-{index}");
            let _ = std::thread::Builder::new().name(name).spawn(move || {
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
        }
        sender
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<GitTask<T>> {
        Self::spawn_with(&self.interactive, wake, run)
    }

    pub fn spawn_operation<T: Send + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<GitTask<T>> {
        Self::spawn_with(&self.operations, wake, run)
    }

    pub fn spawn_background<T: Send + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<GitTask<T>> {
        Self::spawn_with(&self.background, wake, run)
    }

    fn spawn_with<T: Send + 'static>(
        sender: &mpsc::SyncSender<Work>,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<GitTask<T>> {
        let state = Arc::new(Mutex::new(None));
        let output = Arc::clone(&state);
        let canceled = Arc::new(AtomicBool::new(false));
        let worker_canceled = Arc::clone(&canceled);
        let work = Box::new(move || {
            if worker_canceled.load(Ordering::Acquire) {
                if let Some(wake) = wake {
                    let _ = wake.send_event(WinitUserEvent::WakeUp);
                }
                return;
            }
            CANCELLATION.with(|current| {
                *current.borrow_mut() = Some(Arc::clone(&worker_canceled));
            });
            let result = catch_unwind(AssertUnwindSafe(run));
            CANCELLATION.with(|current| {
                current.borrow_mut().take();
            });
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
        Some(GitTask { state, canceled })
    }

    pub(super) fn canceled() -> bool {
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
                .is_none_or(GitCancellationGroup::all_canceled)
        })
    }

    pub(super) fn without_cancellation<T>(run: impl FnOnce() -> T) -> T {
        let previous = CANCELLATION.with(|current| current.borrow_mut().take());
        let scope = CancellationScope { previous };
        let output = run();
        drop(scope);
        output
    }
}

#[derive(Clone, Default)]
pub(super) struct GitCancellationGroup(Arc<Mutex<Vec<Weak<AtomicBool>>>>);

impl GitCancellationGroup {
    pub(super) fn join(&self) -> GitCancellationParticipant {
        let canceled = CANCELLATION.with(|current| current.borrow().clone());
        let member = canceled.as_ref().map(Arc::downgrade);
        if let Some(member) = &member
            && let Ok(mut members) = self.0.lock()
        {
            members.push(member.clone());
        }
        GitCancellationParticipant {
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

pub(super) struct GitCancellationParticipant {
    group: GitCancellationGroup,
    member: Option<Weak<AtomicBool>>,
}

impl GitCancellationParticipant {
    pub(super) fn run<T>(&self, run: impl FnOnce() -> T) -> T {
        let previous =
            CANCELLATION_GROUP.with(|current| current.borrow_mut().replace(self.group.clone()));
        let scope = CancellationGroupScope { previous };
        let output = run();
        drop(scope);
        output
    }
}

impl Drop for GitCancellationParticipant {
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
    previous: Option<GitCancellationGroup>,
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

pub struct GitTask<T> {
    state: Arc<Mutex<Option<std::thread::Result<T>>>>,
    canceled: Arc<AtomicBool>,
}

impl<T> GitTask<T> {
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

impl<T> Drop for GitTask<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
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
