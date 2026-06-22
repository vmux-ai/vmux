use std::path::Path;

use vmux_core::event::FileDirEntry;

pub fn list_dir(path: &Path) -> Vec<FileDirEntry> {
    let Ok(read) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut entries: Vec<FileDirEntry> = read
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            FileDirEntry {
                name: e.file_name().to_string_lossy().to_string(),
                path: e.path().to_string_lossy().to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn lists_dir_hides_dotfiles_dirs_first() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("zdir")).unwrap();
        fs::write(tmp.path().join("a.txt"), "x").unwrap();
        fs::write(tmp.path().join(".hidden"), "x").unwrap();
        let entries = list_dir(tmp.path());
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["zdir", "a.txt"]);
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
