use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::webstore::ChromeWebStore;
use vmux_api::extension::{ExtRow, ExtStatus, ExtensionsEvent};

use sha2::{Digest, Sha256};

static INDEX_LOCK: Mutex<()> = Mutex::new(());
const INDEX_VERSION: u32 = 3;
const LEGACY_PROFILE: &str = "personal";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionStore {
    root: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub popup: Option<String>,
    pub icon: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub profile_enabled: BTreeMap<String, bool>,
    #[serde(default)]
    pub profile_pinned: BTreeMap<String, bool>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub optional_permissions: Vec<String>,
    #[serde(default)]
    pub host_permissions: Vec<String>,
    #[serde(default)]
    pub optional_host_permissions: Vec<String>,
    #[serde(default)]
    pub approved_grants: BTreeMap<String, ExtensionGrants>,
    #[serde(default)]
    pub source_hash: String,
    #[serde(default)]
    pub public_key_b64: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionGrants {
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub host_permissions: Vec<String>,
}

impl ExtensionGrants {
    pub fn covers(&self, permissions: &[String], host_permissions: &[String]) -> bool {
        permissions
            .iter()
            .all(|permission| self.permissions.contains(permission))
            && host_permissions
                .iter()
                .all(|permission| self.host_permissions.contains(permission))
    }

    pub fn retain_declared(
        &mut self,
        permissions: &[String],
        optional_permissions: &[String],
        host_permissions: &[String],
        optional_host_permissions: &[String],
    ) {
        self.permissions.retain(|permission| {
            permissions.contains(permission) || optional_permissions.contains(permission)
        });
        self.host_permissions.retain(|permission| {
            host_permissions.contains(permission) || optional_host_permissions.contains(permission)
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnableForProfileResult {
    Updated,
    NeedsApproval,
    NotFound,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    #[serde(default)]
    version: u32,
    pub entries: Vec<ExtEntry>,
    #[serde(skip)]
    migrated: bool,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            entries: Vec::new(),
            migrated: false,
        }
    }
}

impl ExtensionStore {
    pub fn current() -> Self {
        Self::at(vmux_ecs::profile::ProfilePaths::current().extensions())
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn loaded_ids(&self, profile: &str) -> Vec<String> {
        std::fs::read_to_string(self.loaded_path(profile))
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    std::fs::read_to_string(self.root.join("loaded.txt"))
                } else {
                    Err(error)
                }
            })
            .ok()
            .map(|contents| {
                contents
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn save_loaded_ids(&self, profile: &str, ids: &[String]) -> Result<(), String> {
        std::fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let path = self.loaded_path(profile);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, ids.join("\n")).map_err(|error| error.to_string())
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.root.join("packages")
    }

    pub fn runtimes_dir(&self) -> PathBuf {
        self.root.join("runtime")
    }

    pub fn source_dir(&self, id: &str, version: &str) -> PathBuf {
        self.packages_dir().join(id).join(version).join("source")
    }

    pub fn runtime_dir(&self, profile: &str, id: &str) -> PathBuf {
        self.runtimes_dir().join(profile).join(id)
    }

    pub fn source_hash(&self, source: &Path) -> Result<String, String> {
        let mut files = Vec::new();
        Self::collect_files(source, source, &mut files)?;
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut hasher = Sha256::new();
        for (relative, absolute) in files {
            hasher.update(relative.as_bytes());
            hasher.update([0]);
            hasher.update(std::fs::read(absolute).map_err(|error| error.to_string())?);
            hasher.update([0]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn loaded_path(&self, profile: &str) -> PathBuf {
        self.root.join("loaded").join(format!("{profile}.txt"))
    }

    fn collect_files(
        root: &Path,
        current: &Path,
        output: &mut Vec<(String, PathBuf)>,
    ) -> Result<(), String> {
        for entry in std::fs::read_dir(current).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                Self::collect_files(root, &path, output)?;
            } else {
                let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
                output.push((relative.to_string_lossy().replace('\\', "/"), path));
            }
        }
        Ok(())
    }

    fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
        std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
        for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            let target = destination.join(entry.file_name());
            if file_type.is_dir() {
                Self::copy_tree(&entry.path(), &target)?;
            } else if file_type.is_file() {
                std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
            } else {
                return Err(format!(
                    "unsupported legacy package entry: {}",
                    entry.path().display()
                ));
            }
        }
        Ok(())
    }

    fn generated(name: &str) -> bool {
        name == "vmux_patch.js"
            || name == "vmux_shim.js"
            || name == "vmux_shim.json"
            || name.starts_with("vmux_sw_") && name.ends_with(".js")
    }

    fn remove_generated_files(dir: &Path) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                Self::remove_generated_files(&path)?;
            } else if Self::generated(&entry.file_name().to_string_lossy()) {
                std::fs::remove_file(path).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    fn restore_original_worker(dir: &Path) -> Result<(), String> {
        let sidecar_path = dir.join("vmux_shim.json");
        if sidecar_path.exists() {
            let sidecar: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&sidecar_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let original = sidecar
                .get("original")
                .and_then(serde_json::Value::as_str)
                .ok_or("legacy shim sidecar has no original worker")?;
            if Self::generated(original) {
                return Err("legacy shim sidecar points to a generated worker".into());
            }
            let manifest_path = dir.join("manifest.json");
            let mut manifest: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let background = manifest
                .get_mut("background")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or("legacy manifest has no background object")?;
            background.insert(
                "service_worker".into(),
                serde_json::Value::String(original.into()),
            );
            std::fs::write(
                manifest_path,
                serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        }
        Self::remove_generated_files(dir)
    }

    fn validate_source(dir: &Path) -> Result<(), String> {
        let manifest_path = dir.join("manifest.json");
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if !manifest.is_object() {
            return Err("manifest is not an object".into());
        }
        Ok(())
    }

    pub fn migrate_legacy_package(&self, entry: &ExtEntry) -> Result<PathBuf, String> {
        let source = self.source_dir(&entry.id, &entry.version);
        if source.exists() {
            Self::validate_source(&source)?;
            let hash = self.source_hash(&source)?;
            if !entry.source_hash.is_empty() && hash != entry.source_hash {
                return Err(format!("source hash mismatch for {}", entry.id));
            }
            return Ok(source);
        }

        let legacy = self.root.join(&entry.id);
        if !legacy.is_dir() {
            return Err(format!("legacy extension package not found: {}", entry.id));
        }
        let parent = source.parent().ok_or("source directory has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = parent.join("source.tmp");
        if temporary.exists() {
            std::fs::remove_dir_all(&temporary).map_err(|error| error.to_string())?;
        }
        Self::copy_tree(&legacy, &temporary)?;
        Self::restore_original_worker(&temporary)?;
        Self::validate_source(&temporary)?;
        self.source_hash(&temporary)?;
        std::fs::rename(&temporary, &source).map_err(|error| error.to_string())?;
        Ok(source)
    }
}

impl Index {
    pub fn requires_save(&self) -> bool {
        self.migrated
    }

    pub fn upsert(&mut self, e: ExtEntry) {
        if let Some(slot) = self.entries.iter_mut().find(|x| x.id == e.id) {
            *slot = e;
        } else {
            self.entries.push(e);
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|x| x.id != id);
    }

    pub fn set_enabled_for(
        &mut self,
        profile: &str,
        id: &str,
        enabled: bool,
        approve_permissions: bool,
    ) -> EnableForProfileResult {
        let Some(slot) = self.entries.iter_mut().find(|x| x.id == id) else {
            return EnableForProfileResult::NotFound;
        };
        if !slot.installed_for(profile) {
            return EnableForProfileResult::NotFound;
        }
        if !enabled {
            slot.profile_enabled.insert(profile.to_string(), false);
            return EnableForProfileResult::Updated;
        }
        if !slot
            .grants_for(profile)
            .covers(&slot.permissions, &slot.host_permissions)
        {
            if !approve_permissions {
                return EnableForProfileResult::NeedsApproval;
            }
            slot.approved_grants.insert(
                profile.to_string(),
                ExtensionGrants {
                    permissions: slot.permissions.clone(),
                    host_permissions: slot.host_permissions.clone(),
                },
            );
        }
        slot.profile_enabled.insert(profile.to_string(), true);
        EnableForProfileResult::Updated
    }

    pub fn set_pinned_for(&mut self, profile: &str, id: &str, pinned: bool) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        if !entry.installed_for(profile) {
            return false;
        }
        if entry.pinned_for(profile) == pinned {
            return false;
        }
        entry.profile_pinned.insert(profile.to_string(), pinned);
        true
    }

    pub fn enabled_ids_for(&self, profile: &str) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled_for(profile))
            .map(|e| e.id.clone())
            .collect()
    }

    pub fn enabled_dirs_for(&self, store: &ExtensionStore, profile: &str) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled_for(profile))
            .map(|entry| store.source_dir(&entry.id, &entry.version))
            .collect()
    }

    pub fn is_dirty_for(&self, profile: &str, loaded: &[String]) -> bool {
        let mut a = self.enabled_ids_for(profile);
        let mut b = loaded.to_vec();
        a.sort();
        b.sort();
        a != b
    }

    pub fn snapshot(&self, profile: &str, loaded: &[String]) -> ExtensionsEvent {
        let mut extensions = Vec::new();
        for entry in &self.entries {
            if !entry.installed_for(profile) {
                continue;
            }
            let enabled = entry.enabled_for(profile);
            extensions.push(ExtRow {
                id: entry.id.clone(),
                name: entry.name.clone(),
                version: entry.version.clone(),
                icon: entry.icon.clone(),
                popup: entry.popup.clone(),
                enabled,
                pinned: entry.pinned_for(profile),
                needs_approval: !entry
                    .grants_for(profile)
                    .covers(&entry.permissions, &entry.host_permissions),
                required_permissions: entry.permissions.clone(),
                required_host_permissions: entry.host_permissions.clone(),
                status: if enabled {
                    ExtStatus::Installed
                } else {
                    ExtStatus::Disabled
                },
            });
        }
        ExtensionsEvent {
            loaded: true,
            extensions,
            installing: Vec::new(),
            pending: self.is_dirty_for(profile, loaded),
        }
    }
}

impl ExtEntry {
    pub fn installed_for(&self, profile: &str) -> bool {
        self.profile_enabled.contains_key(profile)
    }

    pub fn enabled_for(&self, profile: &str) -> bool {
        self.profile_enabled.get(profile).copied().unwrap_or(false)
    }

    pub fn pinned_for(&self, profile: &str) -> bool {
        self.profile_pinned.get(profile).copied().unwrap_or(false)
    }

    pub fn grants_for(&self, profile: &str) -> ExtensionGrants {
        self.approved_grants
            .get(profile)
            .cloned()
            .unwrap_or_default()
    }
}

impl ExtensionStore {
    pub(crate) fn snapshot(&self, profile: &str) -> Result<ExtensionsEvent, String> {
        let index = self.load_index()?;
        let loaded = self.loaded_ids(profile);
        Ok(index.snapshot(profile, &loaded))
    }

    pub fn load_index(&self) -> Result<Index, String> {
        let path = self.root.join("index.json");
        if !path.exists() {
            return Ok(Index::default());
        }
        let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let mut index: Index = serde_json::from_str(&source).map_err(|error| error.to_string())?;
        if index.version < INDEX_VERSION {
            for entry in &mut index.entries {
                if index.version < 2 && entry.profile_enabled.is_empty() {
                    entry
                        .profile_enabled
                        .insert(LEGACY_PROFILE.into(), entry.enabled);
                }
                entry.enabled = false;
                if index.version < 3 {
                    let legacy_grants = ExtensionGrants {
                        permissions: entry.permissions.clone(),
                        host_permissions: entry.host_permissions.clone(),
                    };
                    for profile in entry
                        .profile_enabled
                        .iter()
                        .filter_map(|(profile, enabled)| enabled.then_some(profile.clone()))
                        .collect::<Vec<_>>()
                    {
                        entry
                            .approved_grants
                            .entry(profile)
                            .or_insert_with(|| legacy_grants.clone());
                    }
                }
            }
            index.version = INDEX_VERSION;
            index.migrated = true;
        }
        Ok(index)
    }

    pub fn save_index(&self, index: &Index) -> Result<(), String> {
        std::fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let source = serde_json::to_string_pretty(index).map_err(|error| error.to_string())?;
        std::fs::write(self.root.join("index.json"), source).map_err(|error| error.to_string())
    }

    pub fn update_index(&self, update: impl FnOnce(&mut Index)) -> Result<(), String> {
        let _guard = INDEX_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut index = self.load_index()?;
        update(&mut index);
        self.save_index(&index)
    }

    pub fn update_index_if_changed<T>(
        &self,
        update: impl FnOnce(&mut Index) -> Option<T>,
    ) -> Result<Option<T>, String> {
        let _guard = INDEX_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut index = self.load_index()?;
        let Some(value) = update(&mut index) else {
            return Ok(None);
        };
        self.save_index(&index)?;
        Ok(Some(value))
    }

    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        if ChromeWebStore::extension_id(id).as_deref() != Some(id) {
            return Err(format!("invalid extension id: {id}"));
        }
        let _guard = INDEX_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        for directory in [self.root.join(id), self.packages_dir().join(id)] {
            if directory.exists() {
                std::fs::remove_dir_all(&directory).map_err(|error| error.to_string())?;
            }
        }
        let runtimes = self.runtimes_dir();
        if runtimes.exists() {
            for profile in std::fs::read_dir(&runtimes).map_err(|error| error.to_string())? {
                let profile = profile.map_err(|error| error.to_string())?;
                let runtime = profile.path().join(id);
                if runtime.exists() {
                    std::fs::remove_dir_all(runtime).map_err(|error| error.to_string())?;
                }
            }
        }
        let mut index = self.load_index()?;
        index.remove(id);
        self.save_index(&index)
    }

    pub fn uninstall_for_profile(&self, profile: &str, id: &str) -> Result<(), String> {
        if ChromeWebStore::extension_id(id).as_deref() != Some(id) {
            return Err(format!("invalid extension id: {id}"));
        }
        let _guard = INDEX_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut index = self.load_index()?;
        let Some(entry) = index.entries.iter_mut().find(|entry| entry.id == id) else {
            return Ok(());
        };
        entry.profile_enabled.remove(profile);
        entry.profile_pinned.remove(profile);
        entry.approved_grants.remove(profile);
        let remove_package = entry.profile_enabled.is_empty();
        if remove_package {
            index.remove(id);
        }
        self.save_index(&index)?;
        let runtime = self.runtime_dir(profile, id);
        if runtime.exists() {
            std::fs::remove_dir_all(runtime).map_err(|error| error.to_string())?;
        }
        if remove_package {
            for directory in [self.root.join(id), self.packages_dir().join(id)] {
                if directory.exists() {
                    std::fs::remove_dir_all(&directory).map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, enabled: bool) -> ExtEntry {
        let mut profile_enabled = BTreeMap::new();
        profile_enabled.insert(LEGACY_PROFILE.into(), enabled);
        ExtEntry {
            id: id.into(),
            name: id.into(),
            version: "1".into(),
            popup: None,
            icon: None,
            enabled: false,
            profile_enabled,
            profile_pinned: BTreeMap::new(),
            permissions: Vec::new(),
            optional_permissions: Vec::new(),
            host_permissions: Vec::new(),
            optional_host_permissions: Vec::new(),
            approved_grants: BTreeMap::new(),
            source_hash: String::new(),
            public_key_b64: None,
        }
    }

    #[test]
    fn index_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExtensionStore::at(dir.path());
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        store.save_index(&idx).unwrap();
        let loaded = store.load_index().unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert!(loaded.entries[0].enabled_for("personal"));
    }

    #[test]
    fn upsert_replaces_existing() {
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", false));
        assert_eq!(idx.entries.len(), 1);
        assert!(!idx.entries[0].enabled_for("personal"));
    }

    #[test]
    fn enabled_dirs_reflects_profile_toggle() {
        let root = tempfile::tempdir().unwrap();
        let store = ExtensionStore::at(root.path());
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        idx.upsert(entry("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false));
        idx.entries[0].profile_enabled.insert("work".into(), false);
        idx.entries[1].profile_enabled.insert("work".into(), false);
        assert_eq!(
            idx.set_enabled_for("work", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", true, true),
            EnableForProfileResult::Updated
        );
        let dirs = idx.enabled_dirs_for(&store, "work");
        assert_eq!(dirs.len(), 1);
        assert_eq!(
            dirs[0],
            store.source_dir("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "1")
        );
    }

    #[test]
    fn pinning_is_scoped_to_the_profile() {
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));

        assert!(idx.set_pinned_for("personal", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        assert!(idx.entries[0].pinned_for("personal"));
        assert!(!idx.entries[0].pinned_for("work"));
    }

    #[test]
    fn uninstall_rejects_non_extension_id() {
        let root = tempfile::tempdir().unwrap();
        let store = ExtensionStore::at(root.path());
        assert!(store.uninstall("../evil").is_err());
        assert!(store.uninstall("/etc/passwd").is_err());
        assert!(store.uninstall("short").is_err());
    }

    #[test]
    fn uninstall_removes_packages_and_profile_runtimes() {
        let root = tempfile::tempdir().unwrap();
        let store = ExtensionStore::at(root.path());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let package = store.packages_dir().join(id);
        let personal = store.runtime_dir("personal", id);
        let work = store.runtime_dir("work", id);
        std::fs::create_dir_all(&package).unwrap();
        std::fs::create_dir_all(&personal).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let mut index = Index::default();
        index.upsert(entry(id, true));
        store.save_index(&index).unwrap();

        store.uninstall(id).unwrap();

        assert!(!package.exists());
        assert!(!personal.exists());
        assert!(!work.exists());
        assert!(store.load_index().unwrap().entries.is_empty());
    }

    #[test]
    fn dirty_when_enabled_set_differs_from_loaded() {
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        assert!(idx.is_dirty_for("personal", &[]));
        assert!(!idx.is_dirty_for(
            "personal",
            &["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()]
        ));
    }

    #[test]
    fn profile_overrides_preserve_legacy_default() {
        let mut item = entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true);
        item.profile_enabled.insert("work".into(), false);

        assert!(item.enabled_for("personal"));
        assert!(!item.enabled_for("work"));
        assert!(!item.enabled_for("new-profile"));
    }

    #[test]
    fn approved_grants_do_not_cover_permission_expansion() {
        let grants = ExtensionGrants {
            permissions: vec!["storage".into()],
            host_permissions: vec!["https://example.com/*".into()],
        };

        assert!(grants.covers(&["storage".into()], &["https://example.com/*".into()]));
        assert!(!grants.covers(
            &["storage".into(), "history".into()],
            &["https://example.com/*".into()]
        ));
    }

    #[test]
    fn enabling_requires_permission_approval() {
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let mut item = entry(id, false);
        item.permissions = vec!["storage".into()];
        let mut idx = Index::default();
        idx.upsert(item);

        assert_eq!(
            idx.set_enabled_for("personal", id, true, false),
            EnableForProfileResult::NeedsApproval
        );
        assert!(!idx.entries[0].enabled_for("personal"));
        assert_eq!(
            idx.set_enabled_for("personal", id, true, true),
            EnableForProfileResult::Updated
        );
        assert!(idx.entries[0].enabled_for("personal"));
        assert_eq!(
            idx.entries[0].grants_for("personal").permissions,
            vec!["storage".to_string()]
        );
    }

    #[test]
    fn profile_uninstall_preserves_shared_package_until_unused() {
        let root = tempfile::tempdir().unwrap();
        let store = ExtensionStore::at(root.path());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let package = store.packages_dir().join(id);
        std::fs::create_dir_all(&package).unwrap();
        let mut item = entry(id, true);
        item.profile_enabled.insert("work".into(), true);
        let mut index = Index::default();
        index.upsert(item);
        store.save_index(&index).unwrap();

        store.uninstall_for_profile("personal", id).unwrap();

        let index = store.load_index().unwrap();
        assert_eq!(index.entries.len(), 1);
        assert!(!index.entries[0].installed_for("personal"));
        assert!(index.entries[0].installed_for("work"));
        assert!(package.exists());

        store.uninstall_for_profile("work", id).unwrap();

        assert!(store.load_index().unwrap().entries.is_empty());
        assert!(!package.exists());
    }

    #[test]
    fn legacy_global_enablement_migrates_only_to_personal_profile() {
        let root = tempfile::tempdir().unwrap();
        let store = ExtensionStore::at(root.path());
        std::fs::write(
            root.path().join("index.json"),
            serde_json::json!({
                "entries": [{
                    "id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "name": "legacy",
                    "version": "1",
                    "popup": null,
                    "icon": null,
                    "enabled": true
                }]
            })
            .to_string(),
        )
        .unwrap();

        let index = store.load_index().unwrap();
        let entry = &index.entries[0];

        assert!(index.requires_save());
        assert!(entry.enabled_for("personal"));
        assert!(!entry.enabled_for("work"));
        assert!(!entry.enabled);
    }

    #[test]
    fn migrates_legacy_package_without_generated_files() {
        let root = tempfile::tempdir().unwrap();
        let store = ExtensionStore::at(root.path());
        let entry = entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true);
        let legacy = root.path().join(&entry.id);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(
            legacy.join("manifest.json"),
            serde_json::json!({
                "manifest_version": 3,
                "name": "test",
                "version": entry.version,
                "background": { "service_worker": "vmux_sw_deadbeef.js" },
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(legacy.join("background.js"), "original").unwrap();
        std::fs::write(legacy.join("vmux_patch.js"), "patch").unwrap();
        std::fs::write(legacy.join("vmux_sw_deadbeef.js"), "loader").unwrap();
        std::fs::write(
            legacy.join("vmux_shim.json"),
            serde_json::json!({
                "original": "background.js",
                "loader": "vmux_sw_deadbeef.js",
            })
            .to_string(),
        )
        .unwrap();

        let migrated = store.migrate_legacy_package(&entry).unwrap();
        assert_eq!(migrated, store.source_dir(&entry.id, &entry.version));
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(migrated.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["background"]["service_worker"], "background.js");
        assert!(!migrated.join("vmux_patch.js").exists());
        assert!(!migrated.join("vmux_shim.json").exists());
        assert_eq!(store.source_hash(&migrated).unwrap().len(), 64);
        assert!(legacy.join("vmux_shim.json").exists());
    }
}
