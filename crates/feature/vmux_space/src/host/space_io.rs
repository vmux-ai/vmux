use std::path::{Path, PathBuf};

pub(super) struct SpaceFsIo;

impl SpaceFsIo {
    pub(super) fn read_dir_entries(dir: &Path) -> Vec<(String, PathBuf, bool)> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            out.push((name, entry.path(), kind.is_dir()));
        }
        out
    }

    pub(super) fn read(path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    pub(super) fn read_to_string(path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    pub(super) fn write(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        std::fs::write(path, contents)
    }

    pub(super) fn write_snapshot(path: &Path, data: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let pending = path.with_extension(format!("ron.{}.pending", std::process::id()));
        std::fs::write(&pending, data)?;
        if let Err(error) = std::fs::rename(&pending, path) {
            let _ = std::fs::remove_file(pending);
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn backup_for_reset(
        path: &Path,
        store_version_path: &Path,
        schema_version: u32,
    ) -> std::io::Result<PathBuf> {
        for sequence in 0.. {
            let backup = path.with_extension(format!("ron.pre-v{schema_version}.{sequence}"));
            let mut destination = match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)
            {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let mut source = std::fs::File::open(path)?;
            std::io::copy(&mut source, &mut destination)?;
            destination.sync_all()?;
            match std::fs::File::open(store_version_path) {
                Ok(mut version) => {
                    let mut name = backup.as_os_str().to_owned();
                    name.push(".version");
                    let mut destination = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(PathBuf::from(name))?;
                    std::io::copy(&mut version, &mut destination)?;
                    destination.sync_all()?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            #[cfg(unix)]
            if let Some(parent) = backup.parent() {
                std::fs::File::open(parent)?.sync_all()?;
            }
            std::fs::remove_file(path)?;
            return Ok(backup);
        }
        unreachable!()
    }
}
