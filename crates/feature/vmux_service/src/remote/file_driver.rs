#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub(super) struct PrivateFile(std::path::PathBuf);

impl PrivateFile {
    pub(super) fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self(path.into())
    }

    pub(super) fn write(&self, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        vmux_path::AtomicFile::write(&self.0, contents.as_ref())?;
        #[cfg(unix)]
        std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
}

pub(super) struct RemoteFs;

impl RemoteFs {
    pub(super) fn read_to_string(path: &std::path::Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    pub(super) fn write(path: &std::path::Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)
    }

    pub(super) fn remove_file(path: &std::path::Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }
}
