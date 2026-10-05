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
