use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::task::Waker;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

pub(super) struct Transport {
    watcher: RecommendedWatcher,
    observations: mpsc::Receiver<notify::Result<notify::Event>>,
    wake: Arc<Mutex<Option<Waker>>>,
}

impl Transport {
    pub(super) fn new() -> notify::Result<Self> {
        let (sender, observations) = mpsc::channel();
        let wake: Arc<Mutex<Option<Waker>>> = Arc::new(Mutex::new(None));
        let callback_wake = Arc::clone(&wake);
        let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            let notify = !result
                .as_ref()
                .is_ok_and(|event| matches!(event.kind, notify::EventKind::Access(_)));
            if sender.send(result).is_err() || !notify {
                return;
            }
            let waker = match callback_wake.lock() {
                Ok(wake) => wake.clone(),
                Err(_) => None,
            };
            if let Some(waker) = waker {
                let _ = catch_unwind(AssertUnwindSafe(|| waker.wake()));
            }
        })?;
        Ok(Self {
            watcher,
            observations,
            wake,
        })
    }

    pub(super) fn bind_wake(&self, waker: Waker) {
        if let Ok(mut wake) = self.wake.lock() {
            *wake = Some(waker);
        }
    }

    pub(super) fn watch(&mut self, path: &Path) -> notify::Result<()> {
        self.watcher.watch(path, RecursiveMode::NonRecursive)
    }

    pub(super) fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        self.watcher.unwatch(path)
    }

    pub(super) fn receive(&self) -> Option<notify::Result<notify::Event>> {
        self.observations.try_recv().ok()
    }
}
