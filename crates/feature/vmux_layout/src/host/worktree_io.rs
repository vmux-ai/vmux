#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vmux_git::worktree::CheckoutInfo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PathFingerprint {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CheckoutFingerprint {
    dot_git: PathFingerprint,
    admin_dir: PathBuf,
    common_dir: PathBuf,
    commondir: Option<Vec<u8>>,
    gitdir: Option<Vec<u8>>,
    head: Option<Vec<u8>>,
}

impl PathFingerprint {
    pub(super) fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        })
    }
}

impl CheckoutFingerprint {
    pub(super) fn read(info: &CheckoutInfo) -> Option<Self> {
        let dot_git_path = info.root.join(".git");
        let dot_git = PathFingerprint::read(&dot_git_path)?;
        let admin_dir = Self::git_admin_dir(&info.root)?;
        let commondir = std::fs::read(admin_dir.join("commondir")).ok();
        let gitdir = std::fs::read(admin_dir.join("gitdir")).ok();
        let head = std::fs::read(admin_dir.join("HEAD")).ok();
        let common_dir = match commondir.as_deref() {
            Some(bytes) => {
                let value = std::str::from_utf8(bytes).ok()?.trim();
                let path = PathBuf::from(value);
                let path = if path.is_absolute() {
                    path
                } else {
                    admin_dir.join(path)
                };
                path.canonicalize().ok()?
            }
            None => admin_dir.clone(),
        };
        if common_dir != info.common_dir {
            return None;
        }
        Some(Self {
            dot_git,
            admin_dir,
            common_dir,
            commondir,
            gitdir,
            head,
        })
    }

    fn git_admin_dir(root: &Path) -> Option<PathBuf> {
        let dot_git = root.join(".git");
        if dot_git.is_dir() {
            return dot_git.canonicalize().ok();
        }
        let contents = std::fs::read_to_string(&dot_git).ok()?;
        let path = PathBuf::from(contents.strip_prefix("gitdir:")?.trim());
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        path.canonicalize().ok()
    }
}
