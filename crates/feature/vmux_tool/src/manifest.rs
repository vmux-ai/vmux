use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::task::Waker;

use crate::state::ToolProvider;
use async_lock::{Semaphore, SemaphoreGuardArc};
use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};

use crate::dotfiles::DotfilesManifest;
use crate::mcp::McpManifest;

const MANIFEST_VERSION: u32 = 1;

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolsManifest {
    #[serde(default = "manifest_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub packages: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "McpManifest::is_empty")]
    pub mcp: McpManifest,
    #[serde(default, skip_serializing_if = "DotfilesManifest::is_empty")]
    pub dotfiles: DotfilesManifest,
}

impl Default for ToolsManifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            packages: BTreeMap::new(),
            mcp: McpManifest::default(),
            dotfiles: DotfilesManifest::default(),
        }
    }
}

impl ToolsManifest {
    pub fn contains(&self, provider: &str, name: &str) -> bool {
        self.packages
            .get(provider)
            .is_some_and(|packages| packages.iter().any(|package| package == name))
    }

    pub fn set_package(&mut self, provider: &str, name: &str, enabled: bool) {
        if enabled {
            let packages = self.packages.entry(provider.to_string()).or_default();
            if !packages.iter().any(|package| package == name) {
                packages.push(name.to_string());
            }
        } else if let Some(packages) = self.packages.get_mut(provider) {
            packages.retain(|package| package != name);
            if packages.is_empty() {
                self.packages.remove(provider);
            }
        }
        self.normalize();
    }

    pub fn set_dotfile_package(&mut self, name: &str, enabled: bool) {
        if enabled {
            if !self.dotfiles.packages.iter().any(|package| package == name) {
                self.dotfiles.packages.push(name.to_string());
            }
        } else {
            self.dotfiles.packages.retain(|package| package != name);
        }
        self.normalize();
    }

    pub fn managed_packages(&self, provider: &str) -> BTreeSet<String> {
        self.packages
            .get(provider)
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }

    pub(crate) fn add_packages(&mut self, provider: &str, names: &[String]) -> usize {
        let mut imported = 0;
        for name in names {
            imported += usize::from(!self.contains(provider, name));
            self.set_package(provider, name, true);
        }
        imported
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let mut manifest: Self = toml::from_str(&source).map_err(|error| error.to_string())?;
        if manifest.version != MANIFEST_VERSION {
            return Err(format!(
                "unsupported tools manifest version: {}",
                manifest.version
            ));
        }
        manifest.normalize();
        Ok(manifest)
    }

    pub fn write_to(&self, path: &Path) -> Result<(), String> {
        let mut manifest = self.clone();
        manifest.version = MANIFEST_VERSION;
        manifest.normalize();
        let source = toml::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(path, source.as_bytes()).map_err(|error| error.to_string())
    }

    pub(crate) fn normalize(&mut self) {
        self.packages.retain(|_, packages| {
            packages.sort_by_key(|package| package.to_ascii_lowercase());
            packages.dedup();
            !packages.is_empty()
        });
        self.dotfiles
            .packages
            .sort_by_key(|package| package.to_ascii_lowercase());
        self.dotfiles.packages.dedup();
        self.mcp.servers.remove("vmux");
    }

    pub(crate) fn normalize_names(names: &mut Vec<String>) {
        names.retain(|name| !name.trim().is_empty());
        names.sort_by_key(|name| name.to_ascii_lowercase());
        names.dedup();
    }
}

#[derive(Component, Clone, Debug)]
pub struct ToolStore {
    config_dir: PathBuf,
    home: PathBuf,
    execution: Arc<Execution>,
}

impl PartialEq for ToolStore {
    fn eq(&self, other: &Self) -> bool {
        self.config_dir == other.config_dir && self.home == other.home
    }
}

impl Eq for ToolStore {}

pub(crate) struct Lease {
    execution: Arc<Execution>,
    guard: Option<SemaphoreGuardArc>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.guard.take();
        let waker = self.execution.waker.lock().unwrap().clone();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

#[derive(Debug)]
struct Execution {
    permit: Arc<Semaphore>,
    waker: Mutex<Option<Waker>>,
}

type Executions = Mutex<BTreeMap<PathBuf, Weak<Execution>>>;

static EXECUTIONS: OnceLock<Executions> = OnceLock::new();

impl Execution {
    fn for_root(root: &Path) -> Arc<Self> {
        let root = vmux_path::PathIdentity::resolve(root)
            .as_path()
            .to_path_buf();
        let mut executions = EXECUTIONS.get_or_init(Mutex::default).lock().unwrap();
        executions.retain(|_, execution| execution.strong_count() > 0);
        if let Some(execution) = executions.get(&root).and_then(Weak::upgrade) {
            return execution;
        }
        let execution = Arc::new(Self {
            permit: Arc::new(Semaphore::new(1)),
            waker: Mutex::new(None),
        });
        executions.insert(root, Arc::downgrade(&execution));
        execution
    }
}

impl Default for ToolStore {
    fn default() -> Self {
        Self::current()
    }
}

impl ToolStore {
    pub fn current() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::new(vmux_profile::ProfilePaths::current().config(), home)
    }

    pub fn new(config_dir: impl Into<PathBuf>, home: impl Into<PathBuf>) -> Self {
        let config_dir = config_dir.into();
        let execution = Execution::for_root(&config_dir.join("tools"));
        Self {
            config_dir,
            home: home.into(),
            execution,
        }
    }

    #[cfg(any(all(host, ui), test))]
    pub(crate) fn is_busy(&self) -> bool {
        self.execution.permit.try_acquire().is_none()
    }

    pub(crate) fn lease(&self) -> Option<Lease> {
        Some(Lease {
            execution: Arc::clone(&self.execution),
            guard: Some(self.execution.permit.try_acquire_arc()?),
        })
    }

    #[cfg(any(not(target_os = "ios"), test))]
    pub(crate) async fn lease_async(&self) -> Lease {
        Lease {
            execution: Arc::clone(&self.execution),
            guard: Some(self.execution.permit.acquire_arc().await),
        }
    }

    #[cfg(any(all(host, ui), test))]
    pub(crate) fn bind_waker(&self, waker: Waker) {
        let mut current = self.execution.waker.lock().unwrap();
        if current
            .as_ref()
            .is_some_and(|current| current.will_wake(&waker))
        {
            return;
        }
        *current = Some(waker.clone());
        drop(current);
        if !self.is_busy() {
            waker.wake();
        }
    }

    pub fn root(&self) -> PathBuf {
        self.config_dir.join("tools")
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root().join("tools.toml")
    }

    pub fn brewfile_path(&self) -> PathBuf {
        self.root().join("Brewfile")
    }

    pub fn dotfiles_dir(&self) -> PathBuf {
        self.root().join("dotfiles")
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn expand_user_path(&self, path: &Path) -> Result<PathBuf, String> {
        if let Ok(relative) = path.strip_prefix("~") {
            return Ok(self.home.join(relative));
        }
        if path.is_absolute() {
            Ok(path.to_path_buf())
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .map_err(|error| error.to_string())
        }
    }

    pub fn load(&self) -> Result<ToolsManifest, String> {
        self.migrate_legacy_storage()?;
        let mut manifest = ToolsManifest::read(&self.manifest_path())?;
        let brewfile = self.brewfile_path();
        if brewfile.is_file() {
            manifest.sync_brewfile(&brewfile)?;
        } else {
            self.write_brewfile(&manifest)?;
        }
        Ok(manifest)
    }

    pub fn save(&self, manifest: &ToolsManifest) -> Result<(), String> {
        self.migrate_legacy_storage()?;
        manifest.write_to(&self.manifest_path())?;
        self.write_brewfile(manifest)
    }

    pub fn set_managed_package(
        &self,
        provider: &ToolProvider,
        name: &str,
        managed: bool,
    ) -> Result<(), String> {
        let mut manifest = self.load()?;
        manifest.set_package(provider.id(), name, managed);
        self.save(&manifest)
    }

    pub(crate) fn migrate_legacy_storage(&self) -> Result<(), String> {
        let legacy_root = self.config_dir.join("registry");
        let tools_root = self.config_dir.join("tools");
        if legacy_root.symlink_metadata().is_ok() {
            if tools_root.symlink_metadata().is_ok() {
                return Err(format!(
                    "cannot migrate {} because {} already exists",
                    legacy_root.display(),
                    tools_root.display()
                ));
            }
            Self::rename_for_migration(&legacy_root, &tools_root)?;
        }
        let legacy_manifest = tools_root.join("registry.toml");
        let tools_manifest = tools_root.join("tools.toml");
        if legacy_manifest.symlink_metadata().is_ok() {
            if tools_manifest.symlink_metadata().is_ok() {
                return Err(format!(
                    "cannot migrate {} because {} already exists",
                    legacy_manifest.display(),
                    tools_manifest.display()
                ));
            }
            Self::rename_for_migration(&legacy_manifest, &tools_manifest)?;
        }
        Ok(())
    }

    fn rename_for_migration(source: &Path, destination: &Path) -> Result<(), String> {
        match std::fs::rename(source, destination) {
            Ok(()) => Ok(()),
            Err(_)
                if source.symlink_metadata().is_err() && destination.symlink_metadata().is_ok() =>
            {
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

fn manifest_version() -> u32 {
    MANIFEST_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Notify {
        execution: Weak<Execution>,
        sender: std::sync::mpsc::SyncSender<bool>,
    }

    impl std::task::Wake for Notify {
        fn wake(self: Arc<Self>) {
            let available = self
                .execution
                .upgrade()
                .is_some_and(|execution| execution.permit.try_acquire().is_some());
            let _ = self.sender.try_send(available);
        }
    }

    #[test]
    fn releasing_a_store_lease_notifies_after_the_store_is_available() {
        let root = tempfile::tempdir().unwrap();
        let store = ToolStore::new(root.path(), root.path());
        let other = ToolStore::new(root.path(), root.path());
        let lease = store.lease().unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(8);
        let waker = Waker::from(Arc::new(Notify {
            execution: Arc::downgrade(&other.execution),
            sender,
        }));
        other.bind_waker(waker.clone());
        assert!(receiver.try_recv().is_err());
        drop(lease);
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap()
        );
        other.bind_waker(waker);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn registering_after_lease_release_does_not_lose_the_available_notification() {
        let root = tempfile::tempdir().unwrap();
        let store = ToolStore::new(root.path(), root.path());
        drop(store.lease().unwrap());
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        store.bind_waker(Waker::from(Arc::new(Notify {
            execution: Arc::downgrade(&store.execution),
            sender,
        })));
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap()
        );
    }

    #[test]
    fn waiting_for_the_same_store_from_a_fresh_instance_does_not_block_the_worker_pool() {
        let root = tempfile::tempdir().unwrap();
        let store = ToolStore::new(root.path(), root.path());
        let other = ToolStore::new(root.path(), root.path());
        let held = store.lease().unwrap();
        let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(1).build();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let waiting = pool.spawn(async move {
            started_tx.send(()).unwrap();
            let _lease = other.lease_async().await;
            ready_tx.send(()).unwrap();
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let (progress_tx, progress_rx) = std::sync::mpsc::sync_channel(1);
        let progress = pool.spawn(async move {
            progress_tx.send(()).unwrap();
        });
        progress_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert!(ready_rx.try_recv().is_err());
        drop(held);
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        drop((waiting, progress));
    }
}
