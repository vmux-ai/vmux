use std::fs::{DirEntry, File, Metadata, OpenOptions, Permissions};
use std::path::{Path, PathBuf};

pub(crate) struct EditorFsIo;

#[allow(dead_code)]
impl EditorFsIo {
    pub(crate) fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    pub(crate) fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    pub(crate) fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        std::fs::write(path, contents)
    }

    pub(crate) fn create_dir(path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::create_dir(path)
    }

    pub(crate) fn create_dir_all(path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    pub(crate) fn remove_file(path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }

    pub(crate) fn remove_dir(path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::remove_dir(path)
    }

    pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    pub(crate) fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::rename(from, to)
    }

    pub(crate) fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<u64> {
        std::fs::copy(from, to)
    }

    pub(crate) fn hard_link(
        original: impl AsRef<Path>,
        link: impl AsRef<Path>,
    ) -> std::io::Result<()> {
        std::fs::hard_link(original, link)
    }

    pub(crate) fn open(path: impl AsRef<Path>) -> std::io::Result<File> {
        File::open(path)
    }

    pub(crate) fn create(path: impl AsRef<Path>) -> std::io::Result<File> {
        File::create(path)
    }

    pub(crate) fn create_new(path: impl AsRef<Path>) -> std::io::Result<()> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(|_| ())
    }

    pub(crate) fn canonicalize(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    pub(crate) fn open_with(
        options: &OpenOptions,
        path: impl AsRef<Path>,
    ) -> std::io::Result<File> {
        options.open(path)
    }

    pub(crate) fn read_dir(
        path: impl AsRef<Path>,
    ) -> std::io::Result<Vec<std::io::Result<DirEntry>>> {
        Ok(std::fs::read_dir(path)?.collect())
    }

    pub(crate) fn metadata(path: impl AsRef<Path>) -> std::io::Result<Metadata> {
        std::fs::metadata(path)
    }

    pub(crate) fn symlink_metadata(path: impl AsRef<Path>) -> std::io::Result<Metadata> {
        std::fs::symlink_metadata(path)
    }

    pub(crate) fn set_permissions(
        path: impl AsRef<Path>,
        perm: Permissions,
    ) -> std::io::Result<()> {
        std::fs::set_permissions(path, perm)
    }

    #[cfg(unix)]
    pub(crate) fn symlink(
        original: impl AsRef<Path>,
        link: impl AsRef<Path>,
    ) -> std::io::Result<()> {
        std::os::unix::fs::symlink(original, link)
    }
}
