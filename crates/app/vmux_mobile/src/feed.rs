use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};

use tokio::sync::{Notify, watch};

pub struct Pulse(LazyLock<watch::Sender<u64>>);

impl Pulse {
    pub const fn new() -> Self {
        Self(LazyLock::new(|| watch::channel(0).0))
    }

    pub fn fire(&self) {
        self.0.send_modify(|beat| *beat = beat.wrapping_add(1));
    }

    pub fn watching(&self) -> watch::Receiver<u64> {
        self.0.subscribe()
    }
}

pub struct Feed<T> {
    queued: Mutex<VecDeque<T>>,
    ready: Notify,
}

impl<T> Feed<T> {
    pub const fn new() -> Self {
        Self {
            queued: Mutex::new(VecDeque::new()),
            ready: Notify::const_new(),
        }
    }

    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    pub fn offer(&self, value: T) {
        self.queued
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push_back(value);
        self.ready.notify_one();
    }

    pub fn take(&self) -> Option<T> {
        self.queued
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
    }

    pub async fn next(&self) -> T {
        loop {
            if let Some(taken) = self.take() {
                return taken;
            }
            self.ready.notified().await;
        }
    }
}
