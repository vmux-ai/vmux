use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub(crate) struct DesktopFsIo;

#[allow(dead_code)]
impl DesktopFsIo {
    pub(crate) fn create_dir_all(path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    #[cfg(feature = "recording")]
    pub(crate) fn create_dir(path: &Path) -> std::io::Result<()> {
        std::fs::create_dir(path)
    }

    #[cfg(feature = "recording")]
    pub(crate) fn rename(source: &Path, target: &Path) -> std::io::Result<()> {
        std::fs::rename(source, target)
    }

    #[cfg(feature = "recording")]
    pub(crate) fn file_size(path: &Path) -> std::io::Result<u64> {
        std::fs::metadata(path).map(|metadata| metadata.len())
    }

    #[cfg(feature = "recording")]
    pub(crate) fn remove_file(path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }

    #[cfg(feature = "recording")]
    pub(crate) fn remove_dir(path: &Path) -> std::io::Result<()> {
        std::fs::remove_dir(path)
    }

    #[cfg(feature = "tray")]
    pub(crate) fn read_to_string(path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    pub(crate) fn append_crash(dir: &Path, file_path: &Path, record: &str) {
        let _ = std::fs::create_dir_all(dir);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(file_path) {
            let _ = file.write_all(record.as_bytes());
        }
    }
}
