use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::extension::webstore;

static INDEX_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub popup: Option<String>,
    pub icon: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub source_hash: String,
    #[serde(default)]
    pub public_key_b64: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub entries: Vec<ExtEntry>,
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
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(root.join("index.json"), s).map_err(|e| e.to_string())
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

    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(slot) = self.entries.iter_mut().find(|x| x.id == id) {
            slot.enabled = enabled;
        }
    }

    pub fn enabled_ids(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| e.enabled)
            .map(|e| e.id.clone())
            .collect()
    }

    pub fn enabled_dirs(&self, root: &Path) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| source_dir(root, &entry.id, &entry.version))
            .collect()
    }

    pub fn is_dirty(&self, loaded: &[String]) -> bool {
        let mut a = self.enabled_ids();
        let mut b = loaded.to_vec();
        a.sort();
        b.sort();
        a != b
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, enabled: bool) -> ExtEntry {
        ExtEntry {
            id: id.into(),
            name: id.into(),
            version: "1".into(),
            popup: None,
            icon: None,
            enabled,
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
        assert!(loaded.entries[0].enabled);
    }

    #[test]
    fn upsert_replaces_existing() {
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", false));
        assert_eq!(idx.entries.len(), 1);
        assert!(!idx.entries[0].enabled);
    }

    #[test]
    fn enabled_dirs_reflects_toggle() {
        let root = tempfile::tempdir().unwrap();
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        idx.upsert(entry("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false));
        let dirs = idx.enabled_dirs(root.path());
        assert_eq!(dirs.len(), 1);
        assert_eq!(
            dirs[0],
            source_dir(root.path(), "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "1")
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
        assert!(idx.is_dirty(&[]));
        assert!(!idx.is_dirty(&["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()]));
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
