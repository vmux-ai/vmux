use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub(crate) struct TuiFs;

impl TuiFs {
    #[cfg(feature = "runtime")]
    pub(crate) fn read_to_string(path: &Path) -> Option<String> {
        fs::read_to_string(path).ok()
    }

    pub(crate) fn read_bytes(path: &Path) -> Option<Vec<u8>> {
        fs::read(path).ok()
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn ensure_dir(path: &Path) {
        let _ = fs::create_dir_all(path);
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn remove_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    pub(crate) fn write_atomic_no_symlink(path: &Path, payload: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            if fs::symlink_metadata(parent).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(io::Error::other(
                    "TUI config directory must not be a symlink",
                ));
            }
            fs::create_dir_all(parent)?;
        }
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(io::Error::other("TUI config file must not be a symlink"));
        }
        let temp = path.with_extension(format!("{}.json.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        if let Err(error) = file.write_all(payload) {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        drop(file);
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        Ok(())
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn is_macos_dark_mode() -> bool {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("defaults")
                .args(["read", "-g", "AppleInterfaceStyle"])
                .output();
            output.is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .eq_ignore_ascii_case("dark")
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }
}
