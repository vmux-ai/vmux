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
            let is_dir = e
                .file_type()
                .map(|kind| {
                    kind.is_dir()
                        || kind.is_symlink()
                            && std::fs::metadata(&path)
                                .map(|metadata| metadata.is_dir())
                                .unwrap_or(false)
                })
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
    project_root_with_knowledge(start, &vmux_core::knowledge::knowledge_dir())
}

fn project_root_with_knowledge(start: &Path, knowledge: &Path) -> std::path::PathBuf {
    let base = if start.is_dir() {
        start
    } else {
        start.parent().unwrap_or(start)
    };
    if base.starts_with(knowledge) {
        return knowledge.to_path_buf();
    }
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
#[path = "dir.test.rs"]
mod tests;
