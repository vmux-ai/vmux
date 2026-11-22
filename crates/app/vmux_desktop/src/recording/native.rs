use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::capture_output::CropRect;

#[derive(Component)]
pub(super) struct CapturePorts {
    backend: Arc<dyn OpenCapture>,
    opened: Receiver<Opened>,
    encoded: Receiver<Encoded>,
    completion: Completion,
}

impl CapturePorts {
    pub(super) fn new(wake: Option<super::WakeFn>) -> Self {
        let (opened_tx, opened) = crossbeam_channel::unbounded();
        let (encoded_tx, encoded) = crossbeam_channel::unbounded();
        #[cfg(target_os = "macos")]
        let backend: Arc<dyn OpenCapture> = Arc::new(super::macos::Backend);
        #[cfg(not(target_os = "macos"))]
        let backend: Arc<dyn OpenCapture> = Arc::new(Backend);
        Self {
            backend,
            opened,
            encoded,
            completion: Completion {
                opened: opened_tx,
                encoded: encoded_tx,
                wake,
            },
        }
    }

    pub(super) fn open(&self, request: OpenRequest) -> Result<(), String> {
        self.backend.open(request, self.completion.clone())
    }

    pub(super) fn opened(&self) -> Option<Opened> {
        self.opened.try_recv().ok()
    }

    pub(super) fn encoded(&self) -> Option<Encoded> {
        self.encoded.try_recv().ok()
    }

    pub(super) fn wake(&self) {
        self.completion.wake();
    }

    #[cfg(test)]
    pub(super) fn replace_backend(&mut self, backend: Arc<dyn OpenCapture>) {
        self.backend = backend;
    }
}

pub(super) struct OpenRequest {
    pub attempt: Entity,
    pub window: Entity,
    pub width: u32,
    pub height: u32,
    pub crop: Option<CropRect>,
    pub scale: f64,
    pub bitrate: i32,
    pub frame_rate: u32,
    pub gif_fps: u32,
    pub gif_max_edge: u32,
    pub temp_mp4: PathBuf,
    pub temp_gif: Option<PathBuf>,
}

pub(super) struct Opened {
    pub attempt: Entity,
    pub handle: Option<CaptureHandle>,
    pub result: Result<Instant, String>,
}

pub(super) struct Encoded {
    pub attempt: Entity,
    pub result: Result<u64, String>,
}

#[derive(Clone)]
pub(super) struct Completion {
    opened: Sender<Opened>,
    encoded: Sender<Encoded>,
    wake: Option<super::WakeFn>,
}

impl Completion {
    pub(super) fn opened(&self, receipt: Opened) {
        if let Err(error) = self.opened.send(receipt)
            && let Some(handle) = error.0.handle
        {
            handle.finish();
        }
        self.wake();
    }

    pub(super) fn encoded(&self, receipt: Encoded) {
        let _ = self.encoded.send(receipt);
        self.wake();
    }

    fn wake(&self) {
        if let Some(wake) = &self.wake {
            wake();
        }
    }
}

pub(super) trait OpenCapture: Send + Sync {
    fn open(&self, request: OpenRequest, completion: Completion) -> Result<(), String>;
}

pub(super) trait CaptureControl: Send + Sync {
    fn pause(&self);
    fn resume(&self);
    fn finish(self: Arc<Self>);
}

#[derive(Component, Clone)]
pub(super) struct CaptureHandle(Arc<dyn CaptureControl>);

impl CaptureHandle {
    pub(super) fn new(handle: Arc<dyn CaptureControl>) -> Self {
        Self(handle)
    }

    pub(super) fn pause(&self) {
        self.0.pause();
    }

    pub(super) fn resume(&self) {
        self.0.resume();
    }

    pub(super) fn finish(&self) {
        self.0.clone().finish();
    }
}

#[cfg(not(target_os = "macos"))]
struct Backend;

#[cfg(not(target_os = "macos"))]
impl OpenCapture for Backend {
    fn open(&self, _request: OpenRequest, _completion: Completion) -> Result<(), String> {
        Err("recording is only supported on macOS".into())
    }
}
