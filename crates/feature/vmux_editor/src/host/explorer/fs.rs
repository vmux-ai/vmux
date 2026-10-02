use std::path::{Component, Path, PathBuf};

pub(super) struct ExplorerFs {
    root: PathBuf,
}

impl ExplorerFs {
    pub(super) fn new(root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("Cannot access {}: {error}", root.display()))?;
        Ok(Self { root })
    }

    pub(super) fn create(
        &self,
        parent: &Path,
        name: &str,
        is_dir: bool,
    ) -> Result<PathBuf, String> {
        let name = Self::name(name)?;
        let parent = self.parent(parent)?;
        let target = parent.join(name);
        if target.exists() {
            return Err(format!("{} already exists", target.display()));
        }
        if is_dir {
            std::fs::create_dir(&target)
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map(|_| ())
        }
        .map_err(|error| format!("Cannot create {}: {error}", target.display()))?;
        Ok(target)
    }

    pub(super) fn rename(&self, path: &Path, name: &str) -> Result<(PathBuf, bool), String> {
        let name = Self::name(name)?;
        let source = self.source(path)?;
        let metadata = std::fs::symlink_metadata(&source)
            .map_err(|error| format!("Cannot access {}: {error}", source.display()))?;
        let target = source
            .parent()
            .ok_or_else(|| "Explorer root cannot be changed".to_string())?
            .join(name);
        if target == source {
            return Ok((target, metadata.is_dir()));
        }
        if target.exists() {
            return Err(format!("{} already exists", target.display()));
        }
        std::fs::rename(&source, &target)
            .map_err(|error| format!("Cannot rename {}: {error}", source.display()))?;
        Ok((target, metadata.is_dir()))
    }

    pub(super) fn delete(&self, path: &Path) -> Result<(PathBuf, bool), String> {
        let source = self.source(path)?;
        let metadata = std::fs::symlink_metadata(&source)
            .map_err(|error| format!("Cannot access {}: {error}", source.display()))?;
        let is_dir = metadata.is_dir() && !metadata.file_type().is_symlink();
        if is_dir {
            std::fs::remove_dir_all(&source)
        } else {
            std::fs::remove_file(&source)
        }
        .map_err(|error| format!("Cannot delete {}: {error}", source.display()))?;
        Ok((
            source
                .parent()
                .ok_or_else(|| "Explorer root cannot be changed".to_string())?
                .to_path_buf(),
            is_dir,
        ))
    }

    fn name(name: &str) -> Result<&str, String> {
        let name = name.trim();
        let mut components = Path::new(name).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(_)), None) => Ok(name),
            _ => Err("Name must be one file or folder name".to_string()),
        }
    }

    fn parent(&self, parent: &Path) -> Result<PathBuf, String> {
        let parent = parent
            .canonicalize()
            .map_err(|error| format!("Cannot access {}: {error}", parent.display()))?;
        if !parent.starts_with(&self.root) {
            return Err("Path is outside the Explorer root".to_string());
        }
        Ok(parent)
    }

    fn source(&self, path: &Path) -> Result<PathBuf, String> {
        let parent = path
            .parent()
            .ok_or_else(|| "Explorer root cannot be changed".to_string())?;
        let parent = self.parent(parent)?;
        let name = path
            .file_name()
            .ok_or_else(|| "Explorer root cannot be changed".to_string())?;
        let source = parent.join(name);
        std::fs::symlink_metadata(&source)
            .map_err(|error| format!("Cannot access {}: {error}", source.display()))?;
        Ok(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_renames_and_deletes_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let fs = ExplorerFs::new(root).unwrap();
        let file = fs.create(root, "a.txt", false).unwrap();
        assert!(file.is_file());
        let (renamed, is_dir) = fs.rename(&file, "b.txt").unwrap();
        assert!(!is_dir);
        assert!(renamed.is_file());
        fs.delete(&renamed).unwrap();
        assert!(!renamed.exists());

        let dir = fs.create(root, "src", true).unwrap();
        std::fs::write(dir.join("lib.rs"), "").unwrap();
        let (renamed, is_dir) = fs.rename(&dir, "source").unwrap();
        assert!(is_dir);
        fs.delete(&renamed).unwrap();
        assert!(!renamed.exists());
    }

    #[test]
    fn rejects_nested_names_and_outside_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let fs = ExplorerFs::new(tmp.path()).unwrap();
        assert!(fs.create(tmp.path(), "a/b", false).is_err());
        assert!(fs.create(outside.path(), "x", false).is_err());
    }

    #[test]
    fn rejects_root_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let fs = ExplorerFs::new(tmp.path()).unwrap();
        assert!(fs.rename(tmp.path(), "other").is_err());
        assert!(fs.delete(tmp.path()).is_err());
    }
}
