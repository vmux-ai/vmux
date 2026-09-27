use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::SafeStorageError;

static FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct ProtectedFile {
    root: PathBuf,
    path: PathBuf,
}

impl ProtectedFile {
    pub(crate) fn new(root: PathBuf, path: PathBuf) -> Self {
        Self { root, path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn read(&self) -> Result<Option<Vec<u8>>, SafeStorageError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(SafeStorageError::Io(format!(
                "failed to read {}: {error}",
                self.path.display()
            ))),
        }
    }

    pub(crate) fn write(&self, bytes: &[u8]) -> Result<(), SafeStorageError> {
        let Some(parent) = self.path.parent() else {
            return Err(SafeStorageError::Io(
                "Vmux Safe Storage path has no parent".to_string(),
            ));
        };
        std::fs::create_dir_all(parent).map_err(|error| {
            SafeStorageError::Io(format!("failed to create {}: {error}", parent.display()))
        })?;
        Self::restrict_directory(&self.root)?;
        Self::restrict_directory(parent)?;
        let sequence = FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("safe-storage");
        let temporary = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), sequence));
        let result = Self::write_temporary(&temporary, bytes)
            .and_then(|_| std::fs::rename(&temporary, &self.path).map_err(SafeStorageError::from))
            .and_then(|_| {
                std::fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(SafeStorageError::from)
            });
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|error| {
            SafeStorageError::Io(format!("failed to write {}: {error}", self.path.display()))
        })
    }

    pub(crate) fn remove(&self) -> Result<(), SafeStorageError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(SafeStorageError::Io(format!(
                "failed to remove {}: {error}",
                self.path.display()
            ))),
        }
    }

    fn write_temporary(path: &Path, bytes: &[u8]) -> Result<(), SafeStorageError> {
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(bytes)?;
        file.sync_all().map_err(SafeStorageError::from)
    }

    #[cfg(unix)]
    fn restrict_directory(path: &Path) -> Result<(), SafeStorageError> {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(SafeStorageError::from)
    }

    #[cfg(not(unix))]
    fn restrict_directory(_path: &Path) -> Result<(), SafeStorageError> {
        Ok(())
    }
}

impl From<std::io::Error> for SafeStorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
