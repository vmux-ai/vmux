//! Non-Unix filesystems, which carry neither POSIX mode bits nor symlinks.

use std::path::Path;

impl super::FileAttributes {
    pub(crate) fn mode(_metadata: &std::fs::Metadata) -> u32 {
        0
    }

    pub(crate) fn set_mode(_path: &Path, _mode: u32) -> Result<(), String> {
        Ok(())
    }

    pub(crate) fn symlink_target(_path: &Path) -> Result<Vec<u8>, String> {
        Err("Vault symlinks are not supported on this platform".to_string())
    }

    pub(crate) fn create_symlink(_path: &Path, _target: &[u8]) -> Result<(), String> {
        Err("Vault symlinks are not supported on this platform".to_string())
    }
}
