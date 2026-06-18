use std::path::PathBuf;

pub fn active_profile_name() -> &'static str {
    "personal"
}

fn data_dir_suffix_for(profile: &str) -> PathBuf {
    match profile {
        "release" | "local" => PathBuf::from("Vmux"),
        other => PathBuf::from("Vmux").join(other),
    }
}

fn data_dir_suffix() -> PathBuf {
    data_dir_suffix_for(env!("VMUX_BUILD_PROFILE"))
}

pub fn shared_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").expect("HOME not set");
        PathBuf::from(home)
            .join("Library/Application Support")
            .join(data_dir_suffix())
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir().join(data_dir_suffix())
    }
}

pub fn profile_dir() -> PathBuf {
    shared_data_dir()
        .join("profiles")
        .join(active_profile_name())
}

pub fn session_path() -> PathBuf {
    profile_dir().join("session.ron")
}

pub fn cef_cache_path() -> Option<String> {
    profile_dir().to_str().map(|s| s.to_owned())
}

pub fn default_space_dir() -> PathBuf {
    space_dir("space-1")
}

fn spaces_root(home: &std::path::Path) -> PathBuf {
    home.join(".vmux").join("spaces")
}

fn space_dir_path(home: &std::path::Path, space_id: &str) -> PathBuf {
    spaces_root(home).join(space_id)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn is_empty_dir(path: &std::path::Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

/// Remove `dir` and its now-empty ancestors, stopping at (and never removing)
/// `root`.
fn prune_empty_dirs_up(mut dir: PathBuf, root: &std::path::Path) {
    while dir.as_path() != root && dir.starts_with(root) {
        if !is_empty_dir(&dir) || std::fs::remove_dir(&dir).is_err() {
            break;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => break,
        }
    }
}

pub fn space_dir(space_id: &str) -> PathBuf {
    let dir = space_dir_path(&home_dir(), space_id);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn rename_space_dir_in(home: &std::path::Path, old_id: &str, new_id: &str) {
    if old_id == new_id {
        return;
    }
    let old = space_dir_path(home, old_id);
    let new = space_dir_path(home, new_id);
    if old == new {
        return;
    }
    if let Some(parent) = new.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if old.exists() {
        if is_empty_dir(&new) {
            let _ = std::fs::remove_dir(&new);
        }
        if std::fs::rename(&old, &new).is_ok()
            && let Some(parent) = old.parent()
        {
            prune_empty_dirs_up(parent.to_path_buf(), &spaces_root(home));
        }
    } else if !new.exists() {
        let _ = std::fs::create_dir_all(&new);
    }
}

pub fn rename_space_dir(old_id: &str, new_id: &str) {
    rename_space_dir_in(&home_dir(), old_id, new_id);
}

fn collect_subdirs(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_subdirs(&path, out);
            out.push(path);
        }
    }
}

/// Remove directories under `~/.vmux/spaces/` that no longer match a live space
/// id and are empty. Folders that contain files (or back a live space) are kept.
fn prune_orphan_space_dirs_in(home: &std::path::Path, live: &std::collections::HashSet<String>) {
    let root = spaces_root(home);
    let mut dirs = Vec::new();
    collect_subdirs(&root, &mut dirs);
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in dirs {
        let Ok(rel) = dir.strip_prefix(&root) else {
            continue;
        };
        let rel_id = rel.to_string_lossy().replace('\\', "/");
        if live.contains(&rel_id) {
            continue;
        }
        if is_empty_dir(&dir) {
            let _ = std::fs::remove_dir(&dir);
        }
    }
}

pub fn prune_orphan_space_dirs(live: &std::collections::HashSet<String>) {
    prune_orphan_space_dirs_in(&home_dir(), live);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_suffix_maps_each_profile() {
        assert_eq!(data_dir_suffix_for("release"), PathBuf::from("Vmux"));
        assert_eq!(data_dir_suffix_for("local"), PathBuf::from("Vmux"));
        assert_eq!(
            data_dir_suffix_for("dev"),
            PathBuf::from("Vmux").join("dev")
        );
        assert_eq!(
            data_dir_suffix_for("custom"),
            PathBuf::from("Vmux").join("custom"),
        );
    }

    #[test]
    fn local_and_release_share_one_space() {
        assert_eq!(data_dir_suffix_for("local"), data_dir_suffix_for("release"));
    }

    #[test]
    fn dev_lives_under_the_release_space() {
        let release = data_dir_suffix_for("release");
        let dev = data_dir_suffix_for("dev");
        assert!(dev.starts_with(&release));
        assert_ne!(dev, release);
        assert_eq!(dev.file_name().unwrap(), "dev");
    }

    #[test]
    fn shared_data_dir_ends_with_profile_suffix() {
        assert!(shared_data_dir().ends_with(data_dir_suffix()));
    }

    #[test]
    fn space_dir_is_under_vmux_spaces() {
        assert_eq!(
            space_dir_path(std::path::Path::new("/home/u"), "work"),
            PathBuf::from("/home/u/.vmux/spaces/work")
        );
    }

    #[test]
    fn rename_space_dir_moves_existing_folder() {
        let home = std::env::temp_dir().join(format!("vmux-rndir-mv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(space_dir_path(&home, "old")).unwrap();
        rename_space_dir_in(&home, "old", "new");
        assert!(!space_dir_path(&home, "old").exists());
        assert!(space_dir_path(&home, "new").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn rename_space_dir_creates_folder_when_absent() {
        let home = std::env::temp_dir().join(format!("vmux-rndir-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        rename_space_dir_in(&home, "old", "new");
        assert!(space_dir_path(&home, "new").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn rename_space_dir_creates_nested_path() {
        let home = std::env::temp_dir().join(format!("vmux-rndir-nest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(space_dir_path(&home, "old")).unwrap();
        rename_space_dir_in(&home, "old", "org/new");
        assert!(!space_dir_path(&home, "old").exists());
        assert!(space_dir_path(&home, "org/new").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn rename_prunes_empty_old_parent() {
        let home = std::env::temp_dir().join(format!("vmux-rndir-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(space_dir_path(&home, "org/old")).unwrap();
        rename_space_dir_in(&home, "org/old", "elsewhere");
        assert!(space_dir_path(&home, "elsewhere").is_dir());
        assert!(!space_dir_path(&home, "org/old").exists());
        // the now-empty `org` parent is pruned too
        assert!(!space_dir_path(&home, "org").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn prune_removes_empty_orphans_keeps_live_and_files() {
        let home = std::env::temp_dir().join(format!("vmux-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(space_dir_path(&home, "org/live")).unwrap();
        std::fs::create_dir_all(space_dir_path(&home, "org/orphan")).unwrap();
        std::fs::create_dir_all(space_dir_path(&home, "solo")).unwrap();
        std::fs::create_dir_all(space_dir_path(&home, "keep")).unwrap();
        std::fs::write(space_dir_path(&home, "keep").join("f.txt"), b"x").unwrap();

        let live: std::collections::HashSet<String> =
            ["org/live".to_string()].into_iter().collect();
        prune_orphan_space_dirs_in(&home, &live);

        assert!(space_dir_path(&home, "org/live").is_dir());
        assert!(space_dir_path(&home, "org").is_dir());
        assert!(!space_dir_path(&home, "org/orphan").exists());
        assert!(!space_dir_path(&home, "solo").exists());
        assert!(space_dir_path(&home, "keep").is_dir());
        let _ = std::fs::remove_dir_all(&home);
    }
}
