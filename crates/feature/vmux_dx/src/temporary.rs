use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Result;

pub struct Temporary {
    path: PathBuf,
    retained: Arc<AtomicBool>,
}

impl Temporary {
    pub fn create(parent: &Path, name: &str) -> Result<Self> {
        let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = parent.join(format!("{name}-{}-{time}", std::process::id()));
        fs::create_dir(&path)?;
        fs::set_permissions(&path, Permissions::from_mode(0o700))?;
        Ok(Self {
            path,
            retained: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn retention(&self) -> Arc<AtomicBool> {
        self.retained.clone()
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        if !self.retained.load(Ordering::Acquire) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
