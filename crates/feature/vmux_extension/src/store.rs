use std::path::{Path, PathBuf};

pub use crate::snapshot::{EnableForProfileResult, ExtEntry, ExtensionGrants, Index};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionStore {
    root: PathBuf,
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
        crate::filesystem::FileStore::source_hash(source)
    }

    fn loaded_path(&self, profile: &str) -> PathBuf {
        self.root.join("loaded").join(format!("{profile}.txt"))
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
    pub fn enabled_dirs_for(&self, store: &ExtensionStore, profile: &str) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled_for(profile))
            .map(|entry| store.source_dir(&entry.id, &entry.version))
            .collect()
    }
}

impl ExtensionStore {
    pub fn load_index(&self) -> Result<Index, String> {
        crate::filesystem::FileStore::read(&self.root)
    }

    pub fn save_index(&self, index: &Index) -> Result<(), String> {
        crate::filesystem::FileStore::write(&self.root, index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const LEGACY_PROFILE: &str = "personal";

    impl ExtEntry {
        fn fixture(id: &str, enabled: bool) -> Self {
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
    }

    #[test]
    fn index_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExtensionStore::at(dir.path());
        let mut idx = Index::default();
        idx.upsert(ExtEntry::fixture("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        store.save_index(&idx).unwrap();
        let loaded = store.load_index().unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert!(loaded.entries[0].enabled_for("personal"));
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
        let entry = ExtEntry::fixture("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true);
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
