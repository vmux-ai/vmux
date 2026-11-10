use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Result;

pub struct Temporary {
    path: PathBuf,
}

impl Temporary {
    pub fn create(parent: &Path, name: &str) -> Result<Self> {
        let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = parent.join(format!("{name}-{}-{time}", std::process::id()));
        fs::create_dir(&path)?;
        fs::set_permissions(&path, Permissions::from_mode(0o700))?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
