use std::path::Path;

use vmux_core::event::FileDirEntry;

pub fn list_dir(path: &Path) -> Vec<FileDirEntry> {
    let Ok(read) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut entries: Vec<FileDirEntry> = read
        .flatten()
        .map(|e| {
            let path = e.path();
            let is_dir = std::fs::metadata(&path)
                .map(|m| m.is_dir())
                .unwrap_or(false);
            FileDirEntry {
                name: e.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                is_dir,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

pub fn parent_listing(path: &Path) -> (String, Vec<FileDirEntry>) {
    match path.parent() {
        Some(p) => (p.to_string_lossy().to_string(), list_dir(p)),
        None => (String::new(), Vec::new()),
    }
}

/// Nearest ancestor directory containing a `.git` entry, starting from `start`
/// (or its parent when `start` is a file). Falls back to the containing
/// directory when no git root is found.
pub fn project_root(start: &Path) -> std::path::PathBuf {
    let base = if start.is_dir() {
        start
    } else {
        start.parent().unwrap_or(start)
    };
    let mut dir = base;
    loop {
        if dir.join(".git").exists() {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
    base.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn lists_dir_includes_dotfiles_dirs_first() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("zdir")).unwrap();
        fs::write(tmp.path().join("a.txt"), "x").unwrap();
        fs::write(tmp.path().join(".hidden"), "x").unwrap();
        let entries = list_dir(tmp.path());
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["zdir", ".hidden", "a.txt"]);
    }

    #[test]
    fn project_root_walks_up_to_git_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join(".git")).unwrap();
        let sub = root.join("crates").join("x");
        fs::create_dir_all(&sub).unwrap();
        let file = sub.join("lib.rs");
        fs::write(&file, "x").unwrap();
        assert_eq!(project_root(&file), root);
        assert_eq!(project_root(&sub), root);
    }

    #[test]
    fn project_root_falls_back_to_containing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("nogit");
        fs::create_dir(&sub).unwrap();
        let file = sub.join("a.txt");
        fs::write(&file, "x").unwrap();
        assert_eq!(project_root(&file), sub);
    }

    #[test]
    fn parent_listing_of_nested_is_some_root_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let child = tmp.path().join("child");
        fs::create_dir(&child).unwrap();
        let (pp, pe) = parent_listing(&child);
        assert_eq!(pp, tmp.path().to_string_lossy());
        assert!(pe.iter().any(|e| e.name == "child"));

        let (rp, re) = parent_listing(Path::new("/"));
        assert!(rp.is_empty());
        assert!(re.is_empty());
    }
}
