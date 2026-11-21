use bevy_ecs::prelude::Component;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use vmux_ecs::{BlockingCancellation, Job};

#[derive(Clone)]
pub struct CreateTemporary {
    pub root: PathBuf,
    pub transfer: TemporaryTransfer,
}

impl Job for CreateTemporary {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("temporary directory creation canceled".into());
        }
        std::fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let directory = tempfile::Builder::new()
            .prefix("acp-install")
            .tempdir_in(self.root)
            .map_err(|error| error.to_string())?;
        self.transfer.put(TemporaryDirectory::new(directory)?);
        Ok(())
    }
}

#[derive(Component, Clone, Default)]
pub struct TemporaryTransfer(Arc<Mutex<Option<TemporaryDirectory>>>);

impl TemporaryTransfer {
    fn put(&self, staging: TemporaryDirectory) {
        *self.0.lock().unwrap() = Some(staging);
    }

    pub fn take(&self) -> Option<TemporaryDirectory> {
        self.0.lock().unwrap().take()
    }
}

#[derive(Component, Clone)]
pub struct TemporaryDirectory(Arc<Directory>);

impl TemporaryDirectory {
    fn new(directory: tempfile::TempDir) -> Result<Self, String> {
        let path = directory.path().to_path_buf();
        let (release, wait) = mpsc::sync_channel::<()>(0);
        let worker = std::thread::Builder::new()
            .name("temporary-directory-owner".into())
            .spawn(move || {
                let _ = wait.recv();
                directory.close().map_err(|error| error.to_string())
            })
            .map_err(|error| error.to_string())?;
        Ok(Self(Arc::new(Directory {
            path,
            release: Mutex::new(Some(release)),
            worker: Mutex::new(Some(worker)),
        })))
    }

    pub fn path(&self) -> &Path {
        &self.0.path
    }

    pub fn exclusive(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

struct Directory {
    path: PathBuf,
    release: Mutex<Option<mpsc::SyncSender<()>>>,
    worker: Mutex<Option<JoinHandle<Result<(), String>>>>,
}

impl Directory {
    fn close(&self) -> Result<(), String> {
        self.release.lock().unwrap().take();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            return worker
                .join()
                .map_err(|_| "temporary directory cleanup worker panicked".to_string())?;
        }
        match std::fs::remove_dir_all(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Clone)]
pub struct CloseTemporary {
    pub staging: TemporaryDirectory,
}

impl Job for CloseTemporary {
    type Output = Result<(), String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        self.staging.0.close()
    }
}
