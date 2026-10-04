pub use connect::{RepositoryVisibility, VaultRepository};
pub use recovery::{GeneratedRecoveryKey, RecoveryKeyCreation, VaultRecovery};
pub use status::VaultStatus;
use std::path::{Path, PathBuf};

mod connect;
mod files;
mod keys;
mod recovery;
mod repository;
mod snapshot;
mod status;
mod sync;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultStorage {
    root: PathBuf,
    repository: PathBuf,
}

impl VaultStorage {
    pub fn current() -> Self {
        let profile = super::ProfilePaths::current();
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
