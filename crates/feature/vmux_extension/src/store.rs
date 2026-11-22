use std::path::{Path, PathBuf};

pub use crate::snapshot::{ExtEntry, ExtensionGrants, Index};

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

    pub fn from_root(root: &crate::Root) -> Self {
        Self::at(&root.0)
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn loaded_ids(&self, profile: &str) -> Vec<String> {
        crate::filesystem::FileStore::read_loaded_ids(&self.root, profile)
    }

    pub fn save_loaded_ids(&self, profile: &str, ids: &[String]) -> Result<(), String> {
        crate::filesystem::FileStore::write_loaded_ids(&self.root, profile, ids)
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.root.join("packages")
    }

    pub fn runtimes_dir(&self) -> PathBuf {
        self.root.join("runtime")
    }

    pub fn source_dir(&self, id: &str, version: &str) -> PathBuf {
        crate::filesystem::FileStore::source_dir(&self.root, id, version)
    }

    pub fn runtime_dir(&self, profile: &str, id: &str) -> PathBuf {
        crate::filesystem::FileStore::runtime_dir(&self.root, profile, id)
    }

    pub fn source_hash(&self, source: &Path) -> Result<String, String> {
        crate::filesystem::FileStore::source_hash(source)
    }

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

    impl ExtEntry {
        fn fixture(id: &str, enabled: bool) -> Self {
            let mut profile_enabled = BTreeMap::new();
            profile_enabled.insert("personal".into(), enabled);
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
}
