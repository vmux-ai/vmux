use std::path::{Path, PathBuf};

#[cfg(all(unix, test))]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
const DIRECTORIES: [&str; 5] = ["skills", "memories", "projects", "meetings", "handbook"];
const LEGACY_DIRECTORIES: [&str; 4] = ["decisions", "runbooks", "research", "templates"];

pub fn vault_dir() -> PathBuf {
    vmux_core::knowledge::KnowledgeVault::user().into_root()
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

pub fn ensure_vault_repository(root: &Path) -> Result<(), String> {
    if !root.join(".git").exists() {
        vmux_git::worktree::repository_init(root).map_err(|error| error.0)?;
    }
    vmux_git::worktree::ensure_initial_snapshot(root, "Initialize Knowledge vault")
        .map_err(|error| error.0)
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
    fn initializes_knowledge_as_git_repository() {
        let temp = tempfile::tempdir().unwrap();
        ensure_vault(temp.path()).unwrap();
        std::fs::write(temp.path().join("projects/plan.md"), "# Plan\n").unwrap();
        ensure_vault_repository(temp.path()).unwrap();
        assert!(temp.path().join(".git").is_dir());
        assert!(
            vmux_git::runner::file_statuses(temp.path())
                .unwrap()
                .is_empty()
        );
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
