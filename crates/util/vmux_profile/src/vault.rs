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

use std::path::{Path, PathBuf};

pub use connect::{
    RepositoryVisibility, VaultRepository, connect_folder, connect_github_with_progress,
    connect_remote, create_remote,
};
pub use recovery::{GeneratedRecoveryKey, RecoveryKeyCreation, VaultRecovery};
pub use status::{VaultStatus, status, status_with_repositories};
pub use sync::{initialize, sync};

pub fn root_dir() -> PathBuf {
    super::config_dir()
}

pub fn repository_dir() -> PathBuf {
    super::application_data_dir().join("vault")
}

pub fn is_managed_local_path(path: &Path) -> bool {
    path.strip_prefix(root_dir())
        .ok()
        .is_some_and(|relative| !relative.as_os_str().is_empty() && !sync::ignored_path(relative))
}
