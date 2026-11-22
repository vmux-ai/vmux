use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use super::files::FileAttributes;
use super::repository::VaultRepositoryPath;
use super::snapshot::{
    EntryKind, FileTimestamp, LocalEntry, LocalFingerprint, LocalState, LocalStateEntry,
    SnapshotPath,
};

const IGNORED_ROOTS: [&str; 9] = [
    "agents",
    "extensions",
    "lsp",
    "local",
    "profiles",
    "projects",
    "spaces",
    "workspace",
    "worktrees",
];
const FORMAT_VERSION: u32 = 1;

pub(crate) struct VaultSync;

pub(crate) struct VaultLocalState;

pub(super) struct ManagedPath<'a>(pub(super) &'a Path);

pub(crate) struct LocalPath<'a>(pub(crate) &'a Path);

pub(crate) struct RonMap<'a>(pub(crate) &'a ron::value::Map);

impl VaultSync {
    pub(crate) fn collect(root: &Path) -> Result<BTreeMap<String, LocalEntry>, String> {
        let mut files = BTreeMap::new();
        if !root.exists() {
            return Ok(files);
        }
        collect_directory(root, root, &mut files)?;
        Ok(files)
    }
}

fn collect_directory(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, LocalEntry>,
) -> Result<(), String> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        if ManagedPath(relative).ignored() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_dir() {
            collect_directory(root, &path, files)?;
            continue;
        }
        let kind = if metadata.file_type().is_symlink() {
            EntryKind::Symlink
        } else if metadata.file_type().is_file() {
            EntryKind::File
        } else {
            continue;
        };
        let relative = relative
            .to_str()
            .ok_or_else(|| "Vault paths must be valid UTF-8".to_string())?
            .replace(std::path::MAIN_SEPARATOR, "/");
        SnapshotPath(&relative).validate()?;
        let data = match kind {
            EntryKind::File => std::fs::read(&path).map_err(|error| error.to_string())?,
            EntryKind::Symlink => FileAttributes::symlink_target(&path)?,
        };
        let mode = FileAttributes::mode(&metadata);
        let (modified_secs, modified_nanos) = FileTimestamp(&metadata).get();
        let digest = kind.digest(mode, &data);
        files.insert(
            relative,
            LocalEntry {
                kind,
                mode,
                size: metadata.len(),
                modified_secs,
                modified_nanos,
                data,
                digest,
            },
        );
    }
    Ok(())
}

fn collect_local_fingerprints(root: &Path) -> Result<BTreeMap<String, LocalFingerprint>, String> {
    let mut files = BTreeMap::new();
    if root.exists() {
        collect_fingerprint_directory(root, root, &mut files)?;
    }
    Ok(files)
}

fn collect_fingerprint_directory(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, LocalFingerprint>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        if ManagedPath(relative).ignored() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_dir() {
            collect_fingerprint_directory(root, &path, files)?;
            continue;
        }
        let kind = if metadata.file_type().is_symlink() {
            EntryKind::Symlink
        } else if metadata.file_type().is_file() {
            EntryKind::File
        } else {
            continue;
        };
        let relative = relative
            .to_str()
            .ok_or_else(|| "Vault paths must be valid UTF-8".to_string())?
            .replace(std::path::MAIN_SEPARATOR, "/");
        SnapshotPath(&relative).validate()?;
        let (modified_secs, modified_nanos) = FileTimestamp(&metadata).get();
        files.insert(
            relative,
            LocalFingerprint {
                kind,
                mode: FileAttributes::mode(&metadata),
                size: metadata.len(),
                modified_secs,
                modified_nanos,
            },
        );
    }
    Ok(())
}

impl ManagedPath<'_> {
    pub(super) fn ignored(&self) -> bool {
        if self.0.file_name().is_some_and(|name| name == ".DS_Store") {
            return true;
        }
        let first = self
            .0
            .components()
            .next()
            .and_then(|component| match component {
                Component::Normal(value) => value.to_str(),
                _ => None,
            });
        first.is_some_and(|name| {
            name == ".git" || name == ".vmux-vault" || IGNORED_ROOTS.contains(&name)
        })
    }
}

impl<'a> RonMap<'a> {
    pub(crate) fn get(&self, key: &ron::Value) -> Option<&'a ron::Value> {
        self.0
            .iter()
            .find_map(|(candidate, value)| (candidate == key).then_some(value))
    }
}

impl LocalEntry {
    pub(crate) fn same(left: Option<&Self>, right: Option<&Self>) -> bool {
        match (left, right) {
            (Some(left), Some(right)) => {
                left.digest == right.digest && left.kind == right.kind && left.mode == right.mode
            }
            (None, None) => true,
            _ => false,
        }
    }
}

impl LocalPath<'_> {
    pub(crate) fn remove(&self) -> Result<(), String> {
        let Ok(metadata) = std::fs::symlink_metadata(self.0) else {
            return Ok(());
        };
        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            std::fs::remove_dir_all(self.0).map_err(|error| error.to_string())
        } else {
            std::fs::remove_file(self.0).map_err(|error| error.to_string())
        }
    }
}

impl VaultLocalState {
    pub(crate) fn write(root: &Path, repository: &Path) -> Result<(), String> {
        let files = VaultSync::collect(root)?;
        let state = LocalState {
            version: FORMAT_VERSION,
            files: files
                .into_iter()
                .map(|(path, entry)| LocalStateEntry {
                    path,
                    digest: entry.digest,
                    kind: entry.kind,
                    mode: entry.mode,
                    data: Some(entry.data),
                    size: entry.size,
                    modified_secs: entry.modified_secs,
                    modified_nanos: entry.modified_nanos,
                })
                .collect(),
        };
        let source = ron::ser::to_string(&state).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(
            VaultRepositoryPath::at(repository).state_path(),
            source.as_bytes(),
        )
        .map_err(|error| error.to_string())
    }

    pub(crate) fn baseline(repository: &Path) -> Result<BTreeMap<String, LocalEntry>, String> {
        read_local_state(repository)?
            .into_iter()
            .map(|(path, entry)| {
                let data = entry
                    .data
                    .ok_or_else(|| "Vault baseline needs refresh".to_string())?;
                Ok((
                    path,
                    LocalEntry {
                        kind: entry.kind,
                        mode: entry.mode,
                        size: entry.size,
                        modified_secs: entry.modified_secs,
                        modified_nanos: entry.modified_nanos,
                        data,
                        digest: entry.digest,
                    },
                ))
            })
            .collect::<Result<_, String>>()
    }

    pub(super) fn change_count(root: &Path, repository: &Path) -> Result<u32, String> {
        let local = collect_local_fingerprints(root)?;
        let state = read_local_state(repository).unwrap_or_default();
        let paths = local
            .keys()
            .chain(state.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        Ok(paths
            .into_iter()
            .filter(|path| {
                let local = local.get(path);
                let state = state.get(path);
                match (local, state) {
                    (Some(local), Some(state)) => {
                        local.kind != state.kind
                            || local.mode != state.mode
                            || local.size != state.size
                            || local.modified_secs != state.modified_secs
                            || local.modified_nanos != state.modified_nanos
                    }
                    (None, None) => false,
                    _ => true,
                }
            })
            .count() as u32)
    }
}

fn read_local_state(repository: &Path) -> Result<BTreeMap<String, LocalStateEntry>, String> {
    let source = std::fs::read(VaultRepositoryPath::at(repository).state_path())
        .map_err(|error| error.to_string())?;
    let source = std::str::from_utf8(&source).map_err(|error| error.to_string())?;
    let state = ron::from_str::<LocalState>(source).map_err(|error| error.to_string())?;
    if state.version != FORMAT_VERSION {
        return Err("unsupported Vault local state".to_string());
    }
    Ok(state
        .files
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect())
}
