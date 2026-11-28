use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use vmux_ecs::event::FileDirEntry;

use super::repository::GitError;

pub(crate) struct GitFsIo;

impl GitFsIo {
    pub(crate) fn read_entries(path: &Path) -> Vec<FileDirEntry> {
        let Ok(read) = std::fs::read_dir(path) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            let is_dir = entry
                .file_type()
                .map(|kind| {
                    kind.is_dir()
                        || kind.is_symlink()
                            && std::fs::metadata(&path)
                                .map(|metadata| metadata.is_dir())
                                .unwrap_or(false)
                })
                .unwrap_or(false);
            entries.push(FileDirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                is_dir,
            });
        }
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        entries
    }

    pub(crate) fn source_metadata(path: &Path) -> Option<(u64, u64)> {
        let metadata = std::fs::metadata(path).ok()?;
        Some((metadata.dev(), metadata.ino()))
    }

    pub(crate) fn source_marker(path: &Path) -> Option<(u64, u64, u64, i64, i64)> {
        let metadata = std::fs::metadata(path).ok()?;
        if metadata.is_file() {
            return Some((
                metadata.dev(),
                metadata.ino(),
                metadata.len(),
                metadata.mtime(),
                metadata.mtime_nsec(),
            ));
        }
        Some((metadata.dev(), metadata.ino(), 0, 0, 0))
    }

    pub(crate) fn create_staging_dir(parent: &Path) -> Result<tempfile::TempDir, String> {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        tempfile::Builder::new()
            .prefix(".vmux-project-")
            .tempdir_in(parent)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn open_worktree_lock(path: &Path) -> Result<File, GitError> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|error| GitError(format!("failed to open worktree lock: {error}")))?;
        #[cfg(unix)]
        {
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
                return Err(GitError(format!(
                    "failed to acquire worktree lock: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }
        Ok(file)
    }

    pub(crate) fn git_dir_of(dir: &Path) -> Option<PathBuf> {
        for base in dir.ancestors() {
            let dot = base.join(".git");
            let Ok(meta) = std::fs::symlink_metadata(&dot) else {
                continue;
            };
            if meta.is_dir() {
                return Some(dot);
            }
            if !meta.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&dot).ok()?;
            let named = text.lines().find_map(|line| line.strip_prefix("gitdir:"))?;
            let named = PathBuf::from(named.trim());
            if named.is_absolute() {
                return Some(named);
            }
            return Some(base.join(named));
        }
        None
    }

    pub(crate) fn common_dir_of(git_dir: &Path) -> PathBuf {
        let Ok(text) = std::fs::read_to_string(git_dir.join("commondir")) else {
            return git_dir.to_path_buf();
        };
        let named = PathBuf::from(text.trim());
        if named.is_absolute() {
            return named;
        }
        let joined = git_dir.join(named);
        joined.canonicalize().unwrap_or(joined)
    }

    pub(crate) fn branch_in(git_dir: &Path) -> Option<String> {
        let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
        let named = head.trim().strip_prefix("ref: refs/heads/")?;
        (!named.is_empty()).then(|| named.to_string())
    }

    pub(crate) fn separate_diff_spool(output: &mut File) -> Result<(), GitError> {
        let len = output
            .metadata()
            .map_err(|error| GitError(format!("failed to inspect diff spool: {error}")))?
            .len();
        let mut last = [b'\n'];
        if len > 0 {
            output
                .seek(SeekFrom::End(-1))
                .and_then(|_| output.read_exact(&mut last))
                .map_err(|error| {
                    GitError(format!("failed to inspect diff spool ending: {error}"))
                })?;
        }
        output
            .seek(SeekFrom::End(0))
            .map_err(|error| GitError(format!("failed to append diff spool: {error}")))?;
        if last[0] != b'\n' {
            output
                .write_all(b"\n")
                .map_err(|error| GitError(format!("failed to append diff spool: {error}")))?;
        }
        Ok(())
    }

    pub(crate) fn read_regular_file(
        absolute: &Path,
        max_bytes: u64,
    ) -> Result<Result<Vec<u8>, u64>, GitError> {
        let metadata = std::fs::symlink_metadata(absolute)
            .map_err(|error| GitError(format!("failed to inspect diff file: {error}")))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(GitError("branch diff only reads regular files".to_string()));
        }
        if metadata.len() > max_bytes {
            return Ok(Err(metadata.len()));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW);
        let mut file = options
            .open(absolute)
            .map_err(|error| GitError(format!("failed to open diff file: {error}")))?;
        let opened = file
            .metadata()
            .map_err(|error| GitError(format!("failed to inspect opened diff file: {error}")))?;
        if !opened.file_type().is_file() {
            return Err(GitError("branch diff only reads regular files".to_string()));
        }
        if opened.len() > max_bytes {
            return Ok(Err(opened.len()));
        }
        let mut bytes = Vec::with_capacity(opened.len() as usize);
        Read::by_ref(&mut file)
            .take(max_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| GitError(format!("failed to read diff file: {error}")))?;
        if bytes.len() as u64 > max_bytes {
            return Ok(Err(bytes.len() as u64));
        }
        Ok(Ok(bytes))
    }

    pub(crate) fn clone_and_spawn(
        command: &mut Command,
        file: &mut File,
    ) -> Result<Child, GitError> {
        file.seek(SeekFrom::End(0))
            .map_err(|error| GitError(format!("failed to append diff spool: {error}")))?;
        let stdout = file
            .try_clone()
            .map_err(|error| GitError(format!("failed to open diff spool: {error}")))?;
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout))
            .stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        {
            command.process_group(0);
        }
        command
            .spawn()
            .map_err(|error| GitError(format!("failed to run git: {error}")))
    }
}
