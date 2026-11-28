use std::fs::{self, File, OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub(crate) struct SourceFsIo;

impl SourceFsIo {
    pub(crate) fn walk_source_files(
        path: &Path,
        paths: &mut Vec<PathBuf>,
        depth: usize,
        directories: &mut usize,
        canceled: &impl Fn() -> bool,
    ) -> std::io::Result<()> {
        if canceled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "CLI bootstrap canceled",
            ));
        }
        *directories += 1;
        if depth >= 32 || *directories > 8192 || paths.len() > 8192 {
            return Err(std::io::Error::other(
                "CLI source tree exceeds its traversal bound",
            ));
        }
        for entry in fs::read_dir(path)? {
            if canceled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "CLI bootstrap canceled",
                ));
            }
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                Self::walk_source_files(&entry.path(), paths, depth + 1, directories, canceled)?;
            } else if kind.is_file()
                && matches!(
                    entry.path().extension().and_then(|value| value.to_str()),
                    Some("rs" | "ron")
                )
            {
                paths.push(entry.path());
                if paths.len() > 8192 {
                    return Err(std::io::Error::other(
                        "CLI source tree exceeds its entry bound",
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn open_regular_nofollow(path: &Path) -> std::io::Result<File> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        let file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(std::io::Error::other(
                "CLI compiler input must be a regular file",
            ));
        }
        Ok(file)
    }
}
