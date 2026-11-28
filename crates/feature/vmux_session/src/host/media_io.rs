use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

pub(super) struct MediaFsIo;

impl MediaFsIo {
    pub(super) fn walk_composer_files(
        root: &Path,
        max_scan: usize,
    ) -> Vec<(PathBuf, String, bool)> {
        let mut results = Vec::new();
        let walk = WalkBuilder::new(root)
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .follow_links(false)
            .build();
        for entry in walk.take(max_scan) {
            let Ok(entry) = entry else {
                continue;
            };
            if entry.depth() == 0 {
                continue;
            }
            let Some(kind) = entry.file_type() else {
                continue;
            };
            let Ok(relative) = entry.path().strip_prefix(root) else {
                continue;
            };
            results.push((
                entry.path().to_path_buf(),
                relative.to_string_lossy().into_owned(),
                kind.is_dir(),
            ));
        }
        results
    }

    pub(super) fn file_size(path: &Path) -> Option<u64> {
        let metadata = std::fs::metadata(path).ok()?;
        metadata.is_file().then_some(metadata.len())
    }

    pub(super) fn read_bytes(path: &Path) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }
}
