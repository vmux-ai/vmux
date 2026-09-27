use std::path::{Path, PathBuf};

use super::connect::{VaultRepository, github_identity_and_repositories};
use super::keys::{KeyStore, SilentSystemKeyStore};
use super::recovery::read_recovery_envelope;
use super::repository::{git_optional, read_manifest};
use super::snapshot::state_path;
use super::sync::local_change_count;
use super::{repository_dir, root_dir};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VaultStatus {
    pub root: PathBuf,
    pub initialized: bool,
    pub encrypted: bool,
    pub unlocked: bool,
    pub vault_id: String,
    pub recovery_enabled: bool,
    pub remote: String,
    pub branch: String,
    pub dirty: u32,
    pub ahead: u32,
    pub behind: u32,
    pub github_owner: String,
    pub github_owners: Vec<String>,
    pub repositories: Vec<VaultRepository>,
    pub error: String,
}

impl VaultStatus {
    pub fn snapshot(&self) -> vmux_api::vault::VaultStatusSnapshot {
        use vmux_api::vault::VaultStatusSnapshot;

        let remote = self.sanitized_remote();
        let connected = self.initialized && remote.is_some();
        let provider = if !connected {
            None
        } else {
            remote.as_deref().map(Self::provider)
        };
        VaultStatusSnapshot {
            root: self.root.to_string_lossy().into_owned(),
            connected,
            encrypted: self.encrypted,
            unlocked: self.unlocked,
            recovery_key: self.recovery_enabled,
            automatic_backup: true,
            provider,
            remote,
            branch: self.branch.clone(),
            local_changes: self.dirty,
            ahead: self.ahead,
            behind: self.behind,
            sync_needed: self.dirty > 0 || self.ahead > 0 || self.behind > 0,
        }
    }

    fn sanitized_remote(&self) -> Option<String> {
        if self.remote.is_empty() {
            return None;
        }
        let Ok(mut remote) = url::Url::parse(&self.remote) else {
            if self.remote.contains("://") {
                return None;
            }
            return Some(self.remote.clone());
        };
        if remote.username().is_empty() && remote.password().is_none() {
            return Some(self.remote.clone());
        }
        if remote.set_username("").is_err() || remote.set_password(None).is_err() {
            return None;
        }
        Some(remote.to_string())
    }

    fn provider(remote: &str) -> vmux_api::vault::VaultProvider {
        use vmux_api::vault::VaultProvider;

        if Path::new(remote).is_absolute() {
            return VaultProvider::CloudFolder;
        }
        if Self::is_github_remote(remote) {
            return VaultProvider::Github;
        }
        VaultProvider::Git
    }

    fn is_github_remote(remote: &str) -> bool {
        if let Ok(remote) = url::Url::parse(remote) {
            return remote
                .host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case("github.com"));
        }
        if remote.contains("://") {
            return false;
        }
        let Some((authority, path)) = remote.split_once(':') else {
            return false;
        };
        if authority.is_empty()
            || path.is_empty()
            || authority.contains('/')
            || authority.contains('\\')
        {
            return false;
        }
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        host.eq_ignore_ascii_case("github.com")
    }
}

pub fn status() -> VaultStatus {
    status_paths(&root_dir(), &repository_dir(), &SilentSystemKeyStore)
}

pub fn status_with_repositories() -> VaultStatus {
    let mut status = status();
    if !status.initialized || status.remote.is_empty() {
        match github_identity_and_repositories() {
            Ok((owner, owners, repositories)) => {
                status.github_owner = owner;
                status.github_owners = owners;
                status.repositories = repositories;
            }
            Err(error) => status.error = error,
        }
    }
    status
}

pub(super) fn status_paths<K: KeyStore>(root: &Path, repository: &Path, keys: &K) -> VaultStatus {
    let initialized = repository.join(".git").is_dir();
    let manifest = initialized
        .then(|| read_manifest(repository))
        .transpose()
        .ok()
        .flatten();
    let unlocked = manifest.as_ref().is_some_and(|manifest| {
        state_path(repository).is_file() && keys.load(&manifest.vault_id).is_ok()
    });
    let mut status = VaultStatus {
        root: root.to_path_buf(),
        initialized,
        encrypted: manifest.is_some(),
        unlocked,
        ..VaultStatus::default()
    };
    if let Some(manifest) = manifest {
        status.vault_id = manifest.vault_id.clone();
        match read_recovery_envelope(repository) {
            Ok(envelope) => status.recovery_enabled = envelope.is_some(),
            Err(error) => status.error = error,
        }
    }
    if initialized {
        status.remote = git_optional(repository, &["remote", "get-url", "origin"]);
        status.branch = git_optional(repository, &["branch", "--show-current"]);
        status.dirty = local_change_count(root, repository).unwrap_or(0);
        if !status.remote.is_empty() {
            let counts = git_optional(
                repository,
                &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
            );
            let mut values = counts.split_whitespace();
            status.ahead = values
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            status.behind = values
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
        }
    } else if root.join(".git").is_dir() {
        status.error = "A legacy plaintext Vault was found. Create a new encrypted repository; reusing its remote would leave plaintext in Git history.".to_string();
    }
    status
}
