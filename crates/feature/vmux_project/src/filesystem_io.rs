use std::path::{Path, PathBuf};

use crate::Error;

pub(crate) struct FilesystemIo;

impl FilesystemIo {
    pub(crate) fn is_symlink(path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink())
    }

    pub(crate) fn imported_aliases(
        root: &Path,
        imports: &Path,
        directory: &vmux_path::PathIdentity,
    ) -> Result<Vec<PathBuf>, Error> {
        let entries = match std::fs::read_dir(imports) {
            Ok(entries) => entries,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(Vec::new());
            }
            Err(error) => return Err(Error::Unavailable(error.to_string())),
        };
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| Error::Unavailable(error.to_string()))?;
            let path = entry.path();
            let Ok(target) = std::fs::read_link(&path) else {
                continue;
            };
            let target = if target.is_absolute() {
                target
            } else {
                imports.join(target)
            };
            if vmux_path::PathIdentity::resolve(&target) != *directory {
                continue;
            }
            if let Ok(relative) = path.strip_prefix(root) {
                paths.push(relative.to_path_buf());
            }
        }
        Ok(paths)
    }
}
