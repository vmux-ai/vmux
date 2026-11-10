use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

static WRITER: Mutex<()> = Mutex::new(());

pub(crate) struct ProjectRegistry {
    root: PathBuf,
    data: RegistryData,
}

impl ProjectRegistry {
    pub(crate) fn load(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let path = root.join(".projects.ron");
        let data = match std::fs::read_to_string(path) {
            Ok(source) => ron::from_str(&source).map_err(|error| error.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => RegistryData::default(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            root: root.to_path_buf(),
            data,
        })
    }

    pub(crate) fn get(&self, url: &str) -> Option<&Registration> {
        self.data.projects.get(url)
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&String, &Registration)> {
        self.data.projects.iter()
    }

    pub(crate) fn hidden(&self, url: &str) -> bool {
        self.data.hidden.contains(url)
    }

    pub(crate) fn directory(&self, url: &str) -> Option<PathBuf> {
        self.get(url).map(|entry| entry.directory.clone())
    }

    pub(crate) fn save(
        &mut self,
        url: &str,
        expected: u64,
        name: String,
        directory: PathBuf,
        repository: Option<String>,
    ) -> Result<u64, String> {
        let _guard = WRITER.lock().map_err(|error| error.to_string())?;
        self.data = Self::load(&self.root)?.data;
        let current = self.get(url).map_or(0, Registration::revision);
        if current != expected {
            return Err(vmux_ui::i18n::translate("project-edit-conflict"));
        }
        let revision = current
            .checked_add(1)
            .ok_or_else(|| vmux_ui::i18n::translate("project-edit-conflict"))?;
        let directory = directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !directory.join(".git").exists() {
            return Err(vmux_ui::i18n::translate("project-import-not-repository"));
        }
        for (existing, record) in &self.data.projects {
            if existing != url && record.directory == directory {
                return Err(vmux_ui::i18n::translate("project-already-exists"));
            }
        }
        self.data.hidden.remove(url);
        self.data.projects.insert(
            url.to_string(),
            Registration {
                name,
                directory,
                repository,
                revision,
            },
        );
        self.write()?;
        Ok(revision)
    }

    pub(crate) fn remove(&mut self, url: &str) -> Result<(), String> {
        let _guard = WRITER.lock().map_err(|error| error.to_string())?;
        self.data = Self::load(&self.root)?.data;
        if let Some(entry) = self.data.projects.remove(url) {
            if let Some(source_url) =
                crate::project::ProjectRoute::from_path(&self.root, &entry.directory)
                && !self.data.projects.contains_key(&source_url)
            {
                self.data.hidden.insert(source_url);
            }
            self.data.hidden.insert(url.to_string());
        }
        self.write()
    }

    fn write(&self) -> Result<(), String> {
        let source = ron::ser::to_string(&self.data).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(self.root.join(".projects.ron"), source.as_bytes())
            .map_err(|error| error.to_string())
    }
}

#[derive(Default, Serialize, Deserialize)]
struct RegistryData {
    #[serde(default)]
    projects: BTreeMap<String, Registration>,
    #[serde(default)]
    hidden: BTreeSet<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Registration {
    name: String,
    directory: PathBuf,
    repository: Option<String>,
    revision: u64,
}

impl Registration {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn repository(&self) -> Option<&str> {
        self.repository.as_deref()
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_a_registration_preserves_identity_and_old_repository_files() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir_all(first.join(".git")).unwrap();
        std::fs::create_dir_all(second.join(".git")).unwrap();
        std::fs::write(first.join("work.txt"), "keep").unwrap();
        let url = "vmux://projects/first";
        let mut registry = ProjectRegistry::load(root.path()).unwrap();
        registry
            .save(url, 0, "First".into(), first.clone(), None)
            .unwrap();
        registry
            .save(
                url,
                1,
                "Renamed".into(),
                second.clone(),
                Some("https://github.com/example/repo.git".into()),
            )
            .unwrap();
        let mut restored = ProjectRegistry::load(root.path()).unwrap();
        let record = restored.get(url).unwrap();
        assert_eq!(record.name(), "Renamed");
        assert_eq!(record.directory(), second.canonicalize().unwrap());
        assert_eq!(
            std::fs::read_to_string(first.join("work.txt")).unwrap(),
            "keep"
        );
        assert!(
            restored
                .save(url, 1, "Stale".into(), first.clone(), None)
                .is_err()
        );
        restored.remove(url).unwrap();
        let removed = ProjectRegistry::load(root.path()).unwrap();
        assert!(removed.get(url).is_none());
        assert!(removed.hidden(url));
        assert!(removed.hidden("vmux://projects/second"));
        assert!(first.exists() && second.exists());
    }

    #[test]
    fn corrupt_registry_is_reported_without_overwriting_it() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(".projects.ron");
        std::fs::write(&path, "invalid").unwrap();
        assert!(ProjectRegistry::load(root.path()).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "invalid");
    }
}
