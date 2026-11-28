use std::fs::{DirEntry, File, Metadata};
use std::path::Path;

pub(super) struct ExternalFsIo;

impl ExternalFsIo {
    pub(super) fn open(path: &Path) -> std::io::Result<File> {
        File::open(path)
    }

    pub(super) fn metadata(path: &Path) -> std::io::Result<Metadata> {
        std::fs::metadata(path)
    }

    pub(super) fn symlink_metadata(path: &Path) -> std::io::Result<Metadata> {
        std::fs::symlink_metadata(path)
    }

    pub(super) fn read_dir(path: &Path) -> std::io::Result<Vec<std::io::Result<DirEntry>>> {
        Ok(std::fs::read_dir(path)?.collect())
    }
}
