//! Per-file fold persistence: a RON map of absolute path to collapsed header
//! lines, stored under the profile data directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// On-disk fold state for all files, keyed by canonical absolute path.
#[derive(Default, Serialize, Deserialize)]
pub struct FoldStore {
    pub files: HashMap<String, Vec<u32>>,
}

fn store_path() -> PathBuf {
    vmux_core::profile::profile_dir().join("folds.ron")
}

fn key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

impl FoldStore {
    /// Load the store from disk, or an empty store if absent/corrupt.
    pub fn load() -> Self {
        let Ok(text) = std::fs::read_to_string(store_path()) else {
            return Self::default();
        };
        ron::from_str(&text).unwrap_or_default()
    }

    /// Collapsed header lines saved for `path`.
    pub fn get(&self, path: &Path) -> Vec<u32> {
        self.files.get(&key(path)).cloned().unwrap_or_default()
    }

    /// Record collapsed header lines for `path`; an empty list removes the entry.
    pub fn set(&mut self, path: &Path, collapsed: &[u32]) {
        let k = key(path);
        if collapsed.is_empty() {
            self.files.remove(&k);
        } else {
            let mut v = collapsed.to_vec();
            v.sort_unstable();
            self.files.insert(k, v);
        }
    }

    /// Persist the store to disk atomically.
    pub fn save(&self) {
        let path = store_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = ron::ser::to_string(self) {
            let tmp = path.with_extension("ron.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_roundtrip_and_zero_removes() {
        let mut s = FoldStore::default();
        let p = Path::new("/tmp/vmux-fold-test.rs");
        s.set(p, &[3, 1]);
        assert_eq!(s.get(p), vec![1, 3]);
        s.set(p, &[]);
        assert!(s.get(p).is_empty());
    }
}
