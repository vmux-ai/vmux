//! Filesystem mutations used by Explorer context-menu actions.

use std::path::{Component, Path, PathBuf};

fn checked_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(name),
        _ => Err("Name must be one file or folder name".to_string()),
    }
}

fn checked_parent(root: &Path, parent: &Path) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("Cannot access {}: {e}", root.display()))?;
    let parent = parent
        .canonicalize()
        .map_err(|e| format!("Cannot access {}: {e}", parent.display()))?;
    if !parent.starts_with(&root) {
        return Err("Path is outside the Explorer root".to_string());
    }
    Ok(parent)
}

fn checked_source(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Explorer root cannot be changed".to_string())?;
    let parent = checked_parent(root, parent)?;
    let name = path
        .file_name()
        .ok_or_else(|| "Explorer root cannot be changed".to_string())?;
    let source = parent.join(name);
    std::fs::symlink_metadata(&source)
        .map_err(|e| format!("Cannot access {}: {e}", source.display()))?;
    Ok(source)
}

pub fn create_entry(
    root: &Path,
    parent: &Path,
    name: &str,
    is_dir: bool,
) -> Result<PathBuf, String> {
    let name = checked_name(name)?;
    let parent = checked_parent(root, parent)?;
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
    .map_err(|e| format!("Cannot create {}: {e}", target.display()))?;
    Ok(target)
}

pub fn rename_entry(root: &Path, path: &Path, name: &str) -> Result<(PathBuf, bool), String> {
    let name = checked_name(name)?;
    let source = checked_source(root, path)?;
    let metadata = std::fs::symlink_metadata(&source)
        .map_err(|e| format!("Cannot access {}: {e}", source.display()))?;
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
        .map_err(|e| format!("Cannot rename {}: {e}", source.display()))?;
    Ok((target, metadata.is_dir()))
}

pub fn delete_entry(root: &Path, path: &Path) -> Result<(PathBuf, bool), String> {
    let source = checked_source(root, path)?;
    let metadata = std::fs::symlink_metadata(&source)
        .map_err(|e| format!("Cannot access {}: {e}", source.display()))?;
    let is_dir = metadata.is_dir() && !metadata.file_type().is_symlink();
    if is_dir {
        std::fs::remove_dir_all(&source)
    } else {
        std::fs::remove_file(&source)
    }
    .map_err(|e| format!("Cannot delete {}: {e}", source.display()))?;
    Ok((
        source
            .parent()
            .ok_or_else(|| "Explorer root cannot be changed".to_string())?
            .to_path_buf(),
        is_dir,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_renames_and_deletes_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let file = create_entry(root, root, "a.txt", false).unwrap();
        assert!(file.is_file());
        let (renamed, is_dir) = rename_entry(root, &file, "b.txt").unwrap();
        assert!(!is_dir);
        assert!(renamed.is_file());
        delete_entry(root, &renamed).unwrap();
        assert!(!renamed.exists());

        let dir = create_entry(root, root, "src", true).unwrap();
        std::fs::write(dir.join("lib.rs"), "").unwrap();
        let (renamed, is_dir) = rename_entry(root, &dir, "source").unwrap();
        assert!(is_dir);
        delete_entry(root, &renamed).unwrap();
        assert!(!renamed.exists());
    }

    #[test]
    fn rejects_nested_names_and_outside_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        assert!(create_entry(tmp.path(), tmp.path(), "a/b", false).is_err());
        assert!(create_entry(tmp.path(), outside.path(), "x", false).is_err());
    }

    #[test]
    fn rejects_root_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(rename_entry(tmp.path(), tmp.path(), "other").is_err());
        assert!(delete_entry(tmp.path(), tmp.path()).is_err());
    }
}
