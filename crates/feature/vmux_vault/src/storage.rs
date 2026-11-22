pub use connect::RepositoryVisibility;
pub(crate) use files::FileAttributes;
pub use recovery::{GeneratedRecoveryKey, VaultRecovery};
pub(crate) use recovery::{RECOVERY_DIR, RECOVERY_FILE, RecoveryEnvelope};
pub(crate) use repository::VaultRepositoryPath;
pub(crate) use snapshot::{
    EntryKind, FileTimestamp, Hex, INDEX_FILE, LocalEntry, LocalFingerprint, MANIFEST_FILE,
    OBJECTS_DIR, RemoteManifest, SnapshotPath,
};
pub use status::VaultStatus;
use std::path::{Path, PathBuf};
pub(crate) use sync::{LocalPath, RonMap, VaultLocalState, VaultSync};

mod connect;
mod files;
pub(crate) mod keys;
mod recovery;
mod repository;
mod snapshot;
mod status;
mod sync;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultStorage {
    pub(crate) root: PathBuf,
    pub(crate) repository: PathBuf,
}

impl VaultStorage {
    pub fn current() -> Self {
        let profile = vmux_profile::ProfilePaths::current();
        Self {
            root: profile.config(),
            repository: profile.application_data().join("vault"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn repository(&self) -> &Path {
        &self.repository
    }

    pub fn manages(&self, path: &Path) -> bool {
        path.strip_prefix(&self.root).ok().is_some_and(|relative| {
            !relative.as_os_str().is_empty() && !sync::ManagedPath(relative).ignored()
        })
    }
}
