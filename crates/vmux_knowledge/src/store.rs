use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[cfg(all(unix, test))]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use vmux_core::knowledge::{KnowledgeEntry, KnowledgeTreeEvent, markdown_metadata};

const DIRECTORIES: [&str; 5] = ["skills", "memories", "projects", "meetings", "handbook"];
const LEGACY_DIRECTORIES: [&str; 4] = ["decisions", "runbooks", "research", "templates"];
const MAX_DEPTH: usize = 16;
const MAX_ENTRIES: usize = 2_048;
const MAX_METADATA_BYTES: u64 = 64 * 1024;

pub fn vault_dir() -> PathBuf {
    vmux_core::knowledge::knowledge_dir()
}

pub fn ensure_vault(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    for directory in DIRECTORIES {
        std::fs::create_dir_all(root.join(directory))?;
    }
    for directory in LEGACY_DIRECTORIES {
        let _ = std::fs::remove_dir(root.join(directory));
    }
    #[cfg(unix)]
    for directory in std::iter::once(root.to_path_buf()).chain(
        DIRECTORIES
            .into_iter()
            .map(|directory| root.join(directory)),
    ) {
        let permissions = std::fs::metadata(&directory)?.permissions();
        if permissions.mode() & 0o777 != 0o700 {
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

pub fn build_tree(root: &Path) -> std::io::Result<KnowledgeTreeEvent> {
    ensure_vault(root)?;
    let root = root.canonicalize()?;
    let mut count = 0;
    let mut entries = Vec::new();
    scan_directory(&root, 0, &mut count, &mut entries)?;
    Ok(KnowledgeTreeEvent {
        root: root.to_string_lossy().into_owned(),
        entries,
        error: String::new(),
    })
}

pub fn create_entry(
    root: &Path,
    parent: &Path,
    name: &str,
    is_directory: bool,
) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Cannot access {}: {error}", root.display()))?;
    let parent = parent
        .canonicalize()
        .map_err(|error| format!("Cannot access {}: {error}", parent.display()))?;
    if !parent.starts_with(&root) {
        return Err("Path is outside the Knowledge root".to_string());
    }
    let name = name.trim();
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err("Name must be one file or folder name".to_string());
    }
    let name = if is_directory || is_markdown(Path::new(name)) {
        name.to_string()
    } else {
        format!("{name}.md")
    };
    let target = parent.join(name);
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    if is_directory {
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

fn scan_directory(
    directory: &Path,
    depth: usize,
    count: &mut usize,
    output: &mut Vec<KnowledgeEntry>,
) -> std::io::Result<()> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(directory)?.flatten() {
        if *count >= MAX_ENTRIES {
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            entries.push(KnowledgeEntry {
                name,
                title: String::new(),
                path: path.to_string_lossy().into_owned(),
                parent: directory.to_string_lossy().into_owned(),
                is_directory: true,
            });
        } else if file_type.is_file() && is_markdown(&path) {
            entries.push(KnowledgeEntry {
                name,
                title: markdown_title(&path),
                path: path.to_string_lossy().into_owned(),
                parent: directory.to_string_lossy().into_owned(),
                is_directory: false,
            });
        }
    }
    entries.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    for entry in entries {
        if *count >= MAX_ENTRIES {
            break;
        }
        *count += 1;
        let child_directory = entry.is_directory.then(|| PathBuf::from(&entry.path));
        output.push(entry);
        if depth < MAX_DEPTH
            && let Some(child_directory) = child_directory
        {
            let _ = scan_directory(&child_directory, depth + 1, count, output);
        }
    }
    Ok(())
}

fn markdown_title(path: &Path) -> String {
    let Ok(file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut source = String::new();
    if file
        .take(MAX_METADATA_BYTES)
        .read_to_string(&mut source)
        .is_err()
    {
        return String::new();
    }
    markdown_metadata(&source).title
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md")
                || extension.eq_ignore_ascii_case("markdown")
                || extension.eq_ignore_ascii_case("mdx")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_knowledge_folders() {
        let temp = tempfile::tempdir().unwrap();
        ensure_vault(temp.path()).unwrap();
        for directory in DIRECTORIES {
            assert!(temp.path().join(directory).is_dir());
        }
    }

    #[test]
    fn creates_markdown_notes_and_nested_folders_inside_knowledge() {
        let temp = tempfile::tempdir().unwrap();
        ensure_vault(temp.path()).unwrap();
        let folder = create_entry(temp.path(), temp.path(), "Ideas", true).unwrap();
        let note = create_entry(temp.path(), &folder, "Restaurant", false).unwrap();
        assert!(folder.is_dir());
        assert_eq!(note.file_name().unwrap(), "Restaurant.md");
        assert!(note.is_file());
    }

    #[test]
    fn rejects_knowledge_creation_outside_root_and_nested_names() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        ensure_vault(temp.path()).unwrap();
        assert!(create_entry(temp.path(), outside.path(), "note", false).is_err());
        assert!(create_entry(temp.path(), temp.path(), "nested/note", false).is_err());
    }

    #[test]
    fn removes_empty_legacy_folders_and_preserves_content() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("decisions")).unwrap();
        std::fs::create_dir_all(temp.path().join("runbooks")).unwrap();
        std::fs::write(temp.path().join("runbooks/keep.md"), "# Keep").unwrap();
        ensure_vault(temp.path()).unwrap();
        assert!(!temp.path().join("decisions").exists());
        assert!(temp.path().join("runbooks/keep.md").is_file());
    }

    #[test]
    fn builds_sorted_markdown_tree() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("Projects/Nested")).unwrap();
        std::fs::write(temp.path().join("z.md"), "---\ntitle: Zed\n---\n").unwrap();
        std::fs::write(temp.path().join("a.txt"), "ignored").unwrap();
        std::fs::write(temp.path().join("Projects/Nested/Brief.MDX"), "# Brief").unwrap();
        let tree = build_tree(temp.path()).unwrap();
        assert!(tree.entries.first().unwrap().is_directory);
        assert!(tree.entries.iter().any(|entry| entry.name == "z.md"));
        assert_eq!(
            tree.entries
                .iter()
                .find(|entry| entry.name == "z.md")
                .unwrap()
                .title,
            "Zed"
        );
        assert!(!tree.entries.iter().any(|entry| entry.name == "a.txt"));
        let projects = tree
            .entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case("projects"))
            .unwrap();
        let nested = tree
            .entries
            .iter()
            .find(|entry| entry.parent == projects.path && entry.name == "Nested")
            .unwrap();
        assert!(
            tree.entries
                .iter()
                .any(|entry| entry.parent == nested.path && entry.name == "Brief.MDX")
        );
    }

    #[cfg(unix)]
    #[test]
    fn skips_hidden_entries_and_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("visible")).unwrap();
        std::fs::create_dir_all(temp.path().join(".hidden")).unwrap();
        std::fs::write(temp.path().join("visible/note.md"), "# Note").unwrap();
        std::os::unix::fs::symlink(
            temp.path().join("visible/note.md"),
            temp.path().join("linked.md"),
        )
        .unwrap();
        let tree = build_tree(temp.path()).unwrap();
        assert!(!tree.entries.iter().any(|entry| entry.name == ".hidden"));
        assert!(!tree.entries.iter().any(|entry| entry.name == "linked.md"));
    }

    #[cfg(unix)]
    #[test]
    fn vault_is_private() {
        let temp = tempfile::tempdir().unwrap();
        ensure_vault(temp.path()).unwrap();
        assert_eq!(
            std::fs::metadata(temp.path()).unwrap().mode() & 0o777,
            0o700
        );
    }
}
