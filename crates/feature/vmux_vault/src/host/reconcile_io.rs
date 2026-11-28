use std::path::Path;
use std::process::Command;

use crate::storage::{
    EntryKind, FileAttributes, FileTimestamp, Hex, LocalEntry, LocalFingerprint, LocalPath,
    SnapshotPath,
};

pub(super) struct ReconcileIo;

impl ReconcileIo {
    pub(super) fn merge_text(
        baseline: &[u8],
        local: &[u8],
        remote: &[u8],
        strategy_flag: &str,
    ) -> Result<Vec<u8>, String> {
        if std::str::from_utf8(baseline).is_err()
            || std::str::from_utf8(local).is_err()
            || std::str::from_utf8(remote).is_err()
        {
            return Err("Vault text merge requires UTF-8 files".to_string());
        }
        let directory = std::env::temp_dir().join(format!("vmux-vault-merge-{}", Hex::random(8)?));
        std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let baseline_path = directory.join("baseline");
        let local_path = directory.join("local");
        let remote_path = directory.join("remote");
        let result = (|| {
            std::fs::write(&baseline_path, baseline).map_err(|error| error.to_string())?;
            std::fs::write(&local_path, local).map_err(|error| error.to_string())?;
            std::fs::write(&remote_path, remote).map_err(|error| error.to_string())?;
            let output = Command::new("git")
                .arg("merge-file")
                .arg(strategy_flag)
                .arg("--stdout")
                .arg(&local_path)
                .arg(&baseline_path)
                .arg(&remote_path)
                .output()
                .map_err(|error| format!("failed to merge Vault text: {error}"))?;
            if output.status.code().is_some_and(|code| code <= 127) {
                Ok(output.stdout)
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        })();
        let _ = std::fs::remove_dir_all(directory);
        result
    }

    pub(super) fn verify_fingerprints(
        root: &Path,
        expected: &[(String, Option<LocalFingerprint>)],
    ) -> Result<(), String> {
        for (relative, expected) in expected {
            let path = root.join(relative);
            let observed = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    let kind = if metadata.file_type().is_symlink() {
                        EntryKind::Symlink
                    } else if metadata.file_type().is_file() {
                        EntryKind::File
                    } else {
                        return Err(format!("Vault local file changed during sync: {relative}"));
                    };
                    let (modified_secs, modified_nanos) = FileTimestamp(&metadata).get();
                    Some(LocalFingerprint {
                        kind,
                        mode: FileAttributes::mode(&metadata),
                        size: metadata.len(),
                        modified_secs,
                        modified_nanos,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            let matches = match (expected, observed) {
                (Some(expected), Some(observed)) => {
                    expected.kind == observed.kind
                        && expected.mode == observed.mode
                        && expected.size == observed.size
                        && expected.modified_secs == observed.modified_secs
                        && expected.modified_nanos == observed.modified_nanos
                }
                (None, None) => true,
                _ => false,
            };
            if !matches {
                return Err(format!("Vault local file changed during sync: {relative}"));
            }
        }
        Ok(())
    }

    pub(super) fn apply_mutation(
        root: &Path,
        relative: &str,
        entry: Option<&LocalEntry>,
    ) -> Result<(), String> {
        SnapshotPath(relative).validate()?;
        let path = root.join(relative);
        match entry {
            Some(entry) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                LocalPath(&path).remove()?;
                match entry.kind {
                    EntryKind::File => {
                        vmux_path::AtomicFile::write(&path, &entry.data)
                            .map_err(|error| error.to_string())?;
                        FileAttributes::set_mode(&path, entry.mode)?;
                    }
                    EntryKind::Symlink => FileAttributes::create_symlink(&path, &entry.data)?,
                }
            }
            None => {
                LocalPath(&path).remove()?;
                let mut parent = path.parent();
                while let Some(directory) = parent {
                    if directory == root || !directory.starts_with(root) {
                        break;
                    }
                    if std::fs::remove_dir(directory).is_err() {
                        break;
                    }
                    parent = directory.parent();
                }
            }
        }
        Ok(())
    }
}
