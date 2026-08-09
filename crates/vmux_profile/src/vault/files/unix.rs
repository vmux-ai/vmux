//! Unix filesystems, where mode bits and symlinks are native.

use std::path::Path;

impl super::FileAttributes {
    pub(crate) fn mode(metadata: &std::fs::Metadata) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o777
    }

    pub(crate) fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn symlink_target(path: &Path) -> Result<Vec<u8>, String> {
        use std::os::unix::ffi::OsStrExt;
        Ok(std::fs::read_link(path)
            .map_err(|error| error.to_string())?
            .as_os_str()
            .as_bytes()
            .to_vec())
    }

    pub(crate) fn create_symlink(path: &Path, target: &[u8]) -> Result<(), String> {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        std::os::unix::fs::symlink(OsStr::from_bytes(target), path)
            .map_err(|error| error.to_string())
    }
}
