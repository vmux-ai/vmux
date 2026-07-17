use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::extension::webstore;

static INDEX_LOCK: Mutex<()> = Mutex::new(());
const INDEX_VERSION: u32 = 3;
const LEGACY_PROFILE: &str = "personal";

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

pub fn root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".vmux").join("extensions")
}

pub fn packages_root(root: &Path) -> PathBuf {
    root.join("packages")
}

pub fn runtimes_root(root: &Path) -> PathBuf {
    root.join("runtime")
}

pub fn source_dir(root: &Path, id: &str, version: &str) -> PathBuf {
    packages_root(root).join(id).join(version).join("source")
}

pub fn runtime_profile_dir(root: &Path, profile: &str, id: &str) -> PathBuf {
    runtimes_root(root).join(profile).join(id)
}

pub fn tree_sha256(root: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
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

fn collect_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(current).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
            out.push((relative.to_string_lossy().replace('\\', "/"), path));
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
            copy_tree(&entry.path(), &target)?;
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

fn is_vmux_generated(name: &str) -> bool {
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
            remove_generated_files(&path)?;
        } else if is_vmux_generated(&entry.file_name().to_string_lossy()) {
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
        if is_vmux_generated(original) {
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
    remove_generated_files(dir)
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

pub fn migrate_legacy_package(root: &Path, entry: &ExtEntry) -> Result<PathBuf, String> {
    let source = source_dir(root, &entry.id, &entry.version);
    if source.exists() {
        validate_source(&source)?;
        let hash = tree_sha256(&source)?;
        if !entry.source_hash.is_empty() && hash != entry.source_hash {
            return Err(format!("source hash mismatch for {}", entry.id));
        }
        return Ok(source);
    }

    let legacy = root.join(&entry.id);
    if !legacy.is_dir() {
        return Err(format!("legacy extension package not found: {}", entry.id));
    }
    let parent = source.parent().ok_or("source directory has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join("source.tmp");
    if temporary.exists() {
        std::fs::remove_dir_all(&temporary).map_err(|error| error.to_string())?;
    }
    copy_tree(&legacy, &temporary)?;
    restore_original_worker(&temporary)?;
    validate_source(&temporary)?;
    tree_sha256(&temporary)?;
    std::fs::rename(&temporary, &source).map_err(|error| error.to_string())?;
    Ok(source)
}

impl Index {
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = root.join("index.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut index: Self = serde_json::from_str(&s).map_err(|e| e.to_string())?;
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

    pub fn save(&self, root: &Path) -> Result<(), String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(root.join("index.json"), s).map_err(|e| e.to_string())
    }

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

    pub fn enabled_ids_for(&self, profile: &str) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled_for(profile))
            .map(|e| e.id.clone())
            .collect()
    }

    pub fn enabled_dirs_for(&self, root: &Path, profile: &str) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled_for(profile))
            .map(|entry| source_dir(root, &entry.id, &entry.version))
            .collect()
    }

    pub fn is_dirty_for(&self, profile: &str, loaded: &[String]) -> bool {
        let mut a = self.enabled_ids_for(profile);
        let mut b = loaded.to_vec();
        a.sort();
        b.sort();
        a != b
    }
}

impl ExtEntry {
    pub fn installed_for(&self, profile: &str) -> bool {
        self.profile_enabled.contains_key(profile)
    }

    pub fn enabled_for(&self, profile: &str) -> bool {
        self.profile_enabled.get(profile).copied().unwrap_or(false)
    }

    pub fn grants_for(&self, profile: &str) -> ExtensionGrants {
        self.approved_grants
            .get(profile)
            .cloned()
            .unwrap_or_default()
    }
}

pub fn update_index<F: FnOnce(&mut Index)>(root: &Path, f: F) -> Result<(), String> {
    let _guard = INDEX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut idx = Index::load(root)?;
    f(&mut idx);
    idx.save(root)
}

pub fn uninstall(root: &Path, id: &str) -> Result<(), String> {
    if webstore::extension_id(id).as_deref() != Some(id) {
        return Err(format!("invalid extension id: {id}"));
    }
    let _guard = INDEX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dir in [root.join(id), packages_root(root).join(id)] {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        }
    }
    let runtimes = runtimes_root(root);
    if runtimes.exists() {
        for profile in std::fs::read_dir(&runtimes).map_err(|error| error.to_string())? {
            let profile = profile.map_err(|error| error.to_string())?;
            let runtime = profile.path().join(id);
            if runtime.exists() {
                std::fs::remove_dir_all(runtime).map_err(|error| error.to_string())?;
            }
        }
    }
    let mut idx = Index::load(root)?;
    idx.remove(id);
    idx.save(root)
}

pub fn uninstall_for_profile(root: &Path, profile: &str, id: &str) -> Result<(), String> {
    if webstore::extension_id(id).as_deref() != Some(id) {
        return Err(format!("invalid extension id: {id}"));
    }
    let _guard = INDEX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut idx = Index::load(root)?;
    let Some(entry) = idx.entries.iter_mut().find(|entry| entry.id == id) else {
        return Ok(());
    };
    entry.profile_enabled.remove(profile);
    entry.approved_grants.remove(profile);
    let remove_package = entry.profile_enabled.is_empty();
    if remove_package {
        idx.remove(id);
    }
    idx.save(root)?;
    let runtime = runtime_profile_dir(root, profile, id);
    if runtime.exists() {
        std::fs::remove_dir_all(runtime).map_err(|error| error.to_string())?;
    }
    if remove_package {
        for dir in [root.join(id), packages_root(root).join(id)] {
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
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
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        idx.save(dir.path()).unwrap();
        let loaded = Index::load(dir.path()).unwrap();
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
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        idx.upsert(entry("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false));
        idx.entries[0].profile_enabled.insert("work".into(), false);
        idx.entries[1].profile_enabled.insert("work".into(), false);
        assert_eq!(
            idx.set_enabled_for("work", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", true, true),
            EnableForProfileResult::Updated
        );
        let dirs = idx.enabled_dirs_for(root.path(), "work");
        assert_eq!(dirs.len(), 1);
        assert_eq!(
            dirs[0],
            source_dir(root.path(), "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "1")
        );
    }

    #[test]
    fn uninstall_rejects_non_extension_id() {
        let root = tempfile::tempdir().unwrap();
        assert!(uninstall(root.path(), "../evil").is_err());
        assert!(uninstall(root.path(), "/etc/passwd").is_err());
        assert!(uninstall(root.path(), "short").is_err());
    }

    #[test]
    fn uninstall_removes_packages_and_profile_runtimes() {
        let root = tempfile::tempdir().unwrap();
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let package = packages_root(root.path()).join(id);
        let personal = runtime_profile_dir(root.path(), "personal", id);
        let work = runtime_profile_dir(root.path(), "work", id);
        std::fs::create_dir_all(&package).unwrap();
        std::fs::create_dir_all(&personal).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let mut index = Index::default();
        index.upsert(entry(id, true));
        index.save(root.path()).unwrap();

        uninstall(root.path(), id).unwrap();

        assert!(!package.exists());
        assert!(!personal.exists());
        assert!(!work.exists());
        assert!(Index::load(root.path()).unwrap().entries.is_empty());
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
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let package = packages_root(root.path()).join(id);
        std::fs::create_dir_all(&package).unwrap();
        let mut item = entry(id, true);
        item.profile_enabled.insert("work".into(), true);
        let mut index = Index::default();
        index.upsert(item);
        index.save(root.path()).unwrap();

        uninstall_for_profile(root.path(), "personal", id).unwrap();

        let index = Index::load(root.path()).unwrap();
        assert_eq!(index.entries.len(), 1);
        assert!(!index.entries[0].installed_for("personal"));
        assert!(index.entries[0].installed_for("work"));
        assert!(package.exists());

        uninstall_for_profile(root.path(), "work", id).unwrap();

        assert!(Index::load(root.path()).unwrap().entries.is_empty());
        assert!(!package.exists());
    }

    #[test]
    fn legacy_global_enablement_migrates_only_to_personal_profile() {
        let root = tempfile::tempdir().unwrap();
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

        let index = Index::load(root.path()).unwrap();
        let entry = &index.entries[0];

        assert!(index.requires_save());
        assert!(entry.enabled_for("personal"));
        assert!(!entry.enabled_for("work"));
        assert!(!entry.enabled);
    }

    #[test]
    fn migrates_legacy_package_without_generated_files() {
        let root = tempfile::tempdir().unwrap();
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

        let migrated = migrate_legacy_package(root.path(), &entry).unwrap();
        assert_eq!(migrated, source_dir(root.path(), &entry.id, &entry.version));
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(migrated.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["background"]["service_worker"], "background.js");
        assert!(!migrated.join("vmux_patch.js").exists());
        assert!(!migrated.join("vmux_shim.json").exists());
        assert_eq!(tree_sha256(&migrated).unwrap().len(), 64);
        assert!(legacy.join("vmux_shim.json").exists());
    }
}
