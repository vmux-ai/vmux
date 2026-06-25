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
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub entries: Vec<ExtEntry>,
}

pub fn root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".vmux").join("extensions")
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
        self.enabled_ids()
            .into_iter()
            .map(|id| root.join(id))
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
    let dir = root.join(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
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
        assert!(dirs[0].ends_with("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    }

    #[test]
    fn uninstall_rejects_non_extension_id() {
        let root = tempfile::tempdir().unwrap();
        assert!(uninstall(root.path(), "../evil").is_err());
        assert!(uninstall(root.path(), "/etc/passwd").is_err());
        assert!(uninstall(root.path(), "short").is_err());
    }

    #[test]
    fn dirty_when_enabled_set_differs_from_loaded() {
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        assert!(idx.is_dirty(&[]));
        assert!(!idx.is_dirty(&["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()]));
    }
}
