use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use std::process::Command;

use super::files::FileAttributes;
use super::keys::{KeyStore, SystemKeyStore};
use super::recovery::load_repository_key;
use super::repository::{
    commit_changes, current_branch, ensure_repository, git, git_optional, read_manifest,
    remote_branch, validate_empty_vault_repository, validate_remote_history,
};
use super::snapshot::{
    EntryKind, LocalEntry, LocalFingerprint, LocalState, LocalStateEntry, entry_digest,
    load_encrypted_snapshot, modified_time, random_hex, state_path, validate_relative_path,
    write_encrypted_snapshot,
};
use super::{repository_dir, root_dir};

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

#[derive(Default)]
pub(super) struct ReconcileOutcome {
    pub(super) automatic_merges: usize,
    pub(super) conflict_copies: usize,
}

#[derive(Clone, Copy)]
pub(super) enum TextMergeStrategy {
    Local,
    Union,
}

pub fn sync() -> Result<String, String> {
    sync_paths(&root_dir(), &repository_dir(), &SystemKeyStore)
}

pub fn initialize() -> Result<(), String> {
    initialize_paths(&root_dir(), &repository_dir(), &SystemKeyStore)
}

pub(super) fn sync_paths<K: KeyStore>(
    root: &Path,
    repository: &Path,
    keys: &K,
) -> Result<String, String> {
    if !repository.join(".git").is_dir() {
        return Err("Vault is not connected to Git".to_string());
    }
    if git_optional(repository, &["remote", "get-url", "origin"]).is_empty() {
        return Err("Vault has no origin remote".to_string());
    }
    let manifest = read_manifest(repository)?;
    let key = load_repository_key(repository, keys, &manifest.vault_id)?;
    let baseline = baseline_files(repository).unwrap_or_else(|_| {
        load_encrypted_snapshot(repository, &key)
            .map(|(_, files)| files)
            .unwrap_or_default()
    });
    let branch = current_branch(repository)?;
    for attempt in 0..3 {
        git(repository, &["fetch", "origin"])?;
        if let Some(remote_branch) = remote_branch(repository) {
            validate_remote_history(repository, &remote_branch)?;
            if git(repository, &["merge-base", "HEAD", &remote_branch]).is_err() {
                return Err("Vault remote has unrelated history".to_string());
            }
            git(repository, &["reset", "--hard", &remote_branch])?;
        }
        let (_, remote_files) = load_encrypted_snapshot(repository, &key)?;
        let outcome = reconcile_local(root, &baseline, &remote_files)?;
        let files = collect_local_files(root)?;
        write_encrypted_snapshot(
            repository,
            &manifest.vault_id,
            &key,
            &files,
            Some(&remote_files),
        )?;
        commit_changes(repository, "Sync vmux Vault")?;
        match git(repository, &["push", "-u", "origin", &branch]) {
            Ok(_) => {
                write_local_state(root, repository)?;
                return Ok(sync_message(&outcome));
            }
            Err(error) if attempt < 2 && push_rejected_for_remote_change(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Err("Vault remote kept changing during sync".to_string())
}

pub(super) fn push_rejected_for_remote_change(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("non-fast-forward")
        || error.contains("fetch first")
        || error.contains("failed to push some refs")
}

pub(super) fn sync_message(outcome: &ReconcileOutcome) -> String {
    if outcome.conflict_copies > 0 {
        format!(
            "Vault synced with {} conflicted {}",
            outcome.conflict_copies,
            if outcome.conflict_copies == 1 {
                "copy"
            } else {
                "copies"
            }
        )
    } else if outcome.automatic_merges > 0 {
        format!(
            "Vault synced with {} automatic {}",
            outcome.automatic_merges,
            if outcome.automatic_merges == 1 {
                "merge"
            } else {
                "merges"
            }
        )
    } else {
        "Vault synced".to_string()
    }
}

pub(super) fn initialize_paths<K: KeyStore>(
    root: &Path,
    repository: &Path,
    keys: &K,
) -> Result<(), String> {
    ensure_repository(repository)?;
    let (vault_id, key, previous) = match read_manifest(repository) {
        Ok(manifest) => {
            let key = load_repository_key(repository, keys, &manifest.vault_id)?;
            let previous = load_encrypted_snapshot(repository, &key)
                .ok()
                .map(|(_, files)| files);
            (manifest.vault_id, key, previous)
        }
        Err(_) => {
            validate_empty_vault_repository(repository)?;
            let vault_id = random_hex(16)?;
            let key = keys.create(&vault_id)?;
            (vault_id, key, None)
        }
    };
    let files = collect_local_files(root)?;
    write_encrypted_snapshot(repository, &vault_id, &key, &files, previous.as_ref())?;
    commit_changes(repository, "Initialize vmux Vault")
}

pub(super) fn collect_local_files(root: &Path) -> Result<BTreeMap<String, LocalEntry>, String> {
    let mut files = BTreeMap::new();
    if !root.exists() {
        return Ok(files);
    }
    collect_directory(root, root, &mut files)?;
    Ok(files)
}

pub(super) fn collect_directory(
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
        if ignored_path(relative) {
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
        validate_relative_path(&relative)?;
        let data = match kind {
            EntryKind::File => std::fs::read(&path).map_err(|error| error.to_string())?,
            EntryKind::Symlink => FileAttributes::symlink_target(&path)?,
        };
        let mode = FileAttributes::mode(&metadata);
        let (modified_secs, modified_nanos) = modified_time(&metadata);
        let digest = entry_digest(kind, mode, &data);
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

pub(super) fn collect_local_fingerprints(
    root: &Path,
) -> Result<BTreeMap<String, LocalFingerprint>, String> {
    let mut files = BTreeMap::new();
    if root.exists() {
        collect_fingerprint_directory(root, root, &mut files)?;
    }
    Ok(files)
}

pub(super) fn collect_fingerprint_directory(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, LocalFingerprint>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        if ignored_path(relative) {
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
        validate_relative_path(&relative)?;
        let (modified_secs, modified_nanos) = modified_time(&metadata);
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

pub(super) fn ignored_path(relative: &Path) -> bool {
    if relative.file_name().is_some_and(|name| name == ".DS_Store") {
        return true;
    }
    let first = relative
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

pub(super) fn reconcile_local(
    root: &Path,
    baseline: &BTreeMap<String, LocalEntry>,
    remote: &BTreeMap<String, LocalEntry>,
) -> Result<ReconcileOutcome, String> {
    let local = collect_local_files(root)?;
    let paths = baseline
        .keys()
        .chain(local.keys())
        .chain(remote.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut updates = Vec::new();
    let mut occupied = paths.clone();
    let mut outcome = ReconcileOutcome::default();
    for path in paths {
        let baseline_entry = baseline.get(&path);
        let local_entry = local.get(&path);
        let remote_entry = remote.get(&path);
        let local_changed = !same_entry(local_entry, baseline_entry);
        let remote_changed = !same_entry(remote_entry, baseline_entry);
        if local_changed && remote_changed && !same_entry(local_entry, remote_entry) {
            if let Some(entry) =
                merge_changed_file(&path, baseline_entry, local_entry, remote_entry)?
            {
                updates.push((path, Some(entry)));
                outcome.automatic_merges += 1;
                continue;
            }
            updates.push((path.clone(), remote_entry.cloned()));
            if let Some(local_entry) = local_entry {
                let copy_path = conflict_copy_path(&path, &mut occupied)?;
                updates.push((copy_path, Some(local_entry.clone())));
                outcome.conflict_copies += 1;
            }
        } else if remote_changed && !local_changed {
            updates.push((path, remote_entry.cloned()));
        }
    }
    let mut merged = local.clone();
    for (path, entry) in &updates {
        if let Some(entry) = entry {
            merged.insert(path.clone(), entry.clone());
        } else {
            merged.remove(path);
        }
    }
    validate_file_tree(&merged)?;
    for (path, entry) in updates {
        apply_local_entry(root, &path, entry.as_ref())?;
    }
    Ok(outcome)
}

pub(super) fn merge_changed_file(
    path: &str,
    baseline: Option<&LocalEntry>,
    local: Option<&LocalEntry>,
    remote: Option<&LocalEntry>,
) -> Result<Option<LocalEntry>, String> {
    let (Some(local), Some(remote)) = (local, remote) else {
        return Ok(None);
    };
    if local.kind != EntryKind::File || remote.kind != EntryKind::File {
        return Ok(None);
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let baseline_data = baseline
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| entry.data.as_slice())
        .unwrap_or_default();
    let data = match extension.as_str() {
        "md" | "markdown" => {
            if [baseline_data, local.data.as_slice(), remote.data.as_slice()]
                .into_iter()
                .any(|data| std::str::from_utf8(data).is_err())
            {
                return Ok(None);
            }
            merge_text(
                baseline_data,
                &local.data,
                &remote.data,
                TextMergeStrategy::Union,
            )?
        }
        "ron" => {
            if [baseline_data, local.data.as_slice(), remote.data.as_slice()]
                .into_iter()
                .any(|data| std::str::from_utf8(data).is_err())
            {
                return Ok(None);
            }
            let ron_baseline = if baseline.is_none() {
                b"{}".as_slice()
            } else {
                baseline_data
            };
            let merged = match merge_ron(ron_baseline, &local.data, &remote.data)? {
                Some(merged) => merged,
                None => merge_text(
                    baseline_data,
                    &local.data,
                    &remote.data,
                    TextMergeStrategy::Local,
                )?,
            };
            let source = std::str::from_utf8(&merged).ok();
            if source.is_none_or(|source| ron::from_str::<serde::de::IgnoredAny>(source).is_err()) {
                return Ok(None);
            }
            merged
        }
        "toml" => {
            let Some(merged) = merge_toml(baseline_data, &local.data, &remote.data)? else {
                return Ok(None);
            };
            merged
        }
        "json" => {
            let json_baseline = if baseline.is_none() {
                b"{}".as_slice()
            } else {
                baseline_data
            };
            let Some(merged) = merge_json(json_baseline, &local.data, &remote.data)? else {
                return Ok(None);
            };
            merged
        }
        _ => return Ok(None),
    };
    Ok(Some(entry_with_data(local, data)))
}

pub(super) fn entry_with_data(template: &LocalEntry, data: Vec<u8>) -> LocalEntry {
    let mut entry = template.clone();
    entry.size = data.len() as u64;
    entry.modified_secs = 0;
    entry.modified_nanos = 0;
    entry.digest = entry_digest(entry.kind, entry.mode, &data);
    entry.data = data;
    entry
}

pub(super) fn merge_text(
    baseline: &[u8],
    local: &[u8],
    remote: &[u8],
    strategy: TextMergeStrategy,
) -> Result<Vec<u8>, String> {
    if std::str::from_utf8(baseline).is_err()
        || std::str::from_utf8(local).is_err()
        || std::str::from_utf8(remote).is_err()
    {
        return Err("Vault text merge requires UTF-8 files".to_string());
    }
    let directory = std::env::temp_dir().join(format!("vmux-vault-merge-{}", random_hex(8)?));
    std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let baseline_path = directory.join("baseline");
    let local_path = directory.join("local");
    let remote_path = directory.join("remote");
    let result = (|| {
        std::fs::write(&baseline_path, baseline).map_err(|error| error.to_string())?;
        std::fs::write(&local_path, local).map_err(|error| error.to_string())?;
        std::fs::write(&remote_path, remote).map_err(|error| error.to_string())?;
        let strategy = match strategy {
            TextMergeStrategy::Local => "--ours",
            TextMergeStrategy::Union => "--union",
        };
        let output = Command::new("git")
            .arg("merge-file")
            .arg(strategy)
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

pub(super) fn merge_toml(
    baseline: &[u8],
    local: &[u8],
    remote: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let Ok(baseline) = std::str::from_utf8(baseline) else {
        return Ok(None);
    };
    let Ok(local) = std::str::from_utf8(local) else {
        return Ok(None);
    };
    let Ok(remote) = std::str::from_utf8(remote) else {
        return Ok(None);
    };
    let Ok(baseline) = toml::from_str::<toml::Value>(baseline) else {
        return Ok(None);
    };
    let Ok(local) = toml::from_str::<toml::Value>(local) else {
        return Ok(None);
    };
    let Ok(remote) = toml::from_str::<toml::Value>(remote) else {
        return Ok(None);
    };
    let Some(merged) = merge_toml_value(Some(&baseline), Some(&local), Some(&remote)) else {
        return Ok(None);
    };
    Ok(Some(
        toml::to_string_pretty(&merged)
            .map_err(|error| error.to_string())?
            .into_bytes(),
    ))
}

pub(super) fn merge_ron(
    baseline: &[u8],
    local: &[u8],
    remote: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let Ok(baseline) = ron::from_str::<ron::Value>(std::str::from_utf8(baseline).unwrap_or(""))
    else {
        return Ok(None);
    };
    let Ok(local) = ron::from_str::<ron::Value>(std::str::from_utf8(local).unwrap_or("")) else {
        return Ok(None);
    };
    let Ok(remote) = ron::from_str::<ron::Value>(std::str::from_utf8(remote).unwrap_or("")) else {
        return Ok(None);
    };
    let Some(merged) = merge_ron_value(Some(&baseline), Some(&local), Some(&remote)) else {
        return Ok(None);
    };
    let mut output = ron::ser::to_string_pretty(&merged, ron::ser::PrettyConfig::new())
        .map_err(|error| error.to_string())?
        .into_bytes();
    output.push(b'\n');
    Ok(Some(output))
}

pub(super) fn merge_ron_value(
    baseline: Option<&ron::Value>,
    local: Option<&ron::Value>,
    remote: Option<&ron::Value>,
) -> Option<ron::Value> {
    if ron_value_options_equal(local, remote) {
        return local.cloned();
    }
    if ron_value_options_equal(local, baseline) {
        return remote.cloned();
    }
    if ron_value_options_equal(remote, baseline) {
        return local.cloned();
    }
    match (baseline, local, remote) {
        (
            Some(ron::Value::Map(baseline)),
            Some(ron::Value::Map(local)),
            Some(ron::Value::Map(remote)),
        ) => {
            let keys = baseline
                .keys()
                .chain(local.keys())
                .chain(remote.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            Some(ron::Value::Map(
                keys.into_iter()
                    .filter_map(|key| {
                        merge_ron_value(
                            ron_map_get(baseline, &key),
                            ron_map_get(local, &key),
                            ron_map_get(remote, &key),
                        )
                        .map(|value| (key, value))
                    })
                    .collect(),
            ))
        }
        _ => local.cloned(),
    }
}

pub(super) fn ron_value_options_equal(
    left: Option<&ron::Value>,
    right: Option<&ron::Value>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => ron_values_equal(left, right),
        (None, None) => true,
        _ => false,
    }
}

pub(super) fn ron_values_equal(left: &ron::Value, right: &ron::Value) -> bool {
    match (left, right) {
        (ron::Value::Map(left), ron::Value::Map(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    ron_map_get(right, key).is_some_and(|other| ron_values_equal(value, other))
                })
        }
        (ron::Value::Seq(left), ron::Value::Seq(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| ron_values_equal(left, right))
        }
        (ron::Value::Option(left), ron::Value::Option(right)) => {
            ron_value_options_equal(left.as_deref(), right.as_deref())
        }
        _ => left == right,
    }
}

pub(super) fn ron_map_get<'a>(
    map: &'a ron::value::Map,
    key: &ron::Value,
) -> Option<&'a ron::Value> {
    map.iter()
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
}

pub(super) fn merge_toml_value(
    baseline: Option<&toml::Value>,
    local: Option<&toml::Value>,
    remote: Option<&toml::Value>,
) -> Option<toml::Value> {
    if local == remote {
        return local.cloned();
    }
    if local == baseline {
        return remote.cloned();
    }
    if remote == baseline {
        return local.cloned();
    }
    match (baseline, local, remote) {
        (
            Some(toml::Value::Table(baseline)),
            Some(toml::Value::Table(local)),
            Some(toml::Value::Table(remote)),
        ) => {
            let keys = baseline
                .keys()
                .chain(local.keys())
                .chain(remote.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            Some(toml::Value::Table(
                keys.into_iter()
                    .filter_map(|key| {
                        merge_toml_value(baseline.get(&key), local.get(&key), remote.get(&key))
                            .map(|value| (key, value))
                    })
                    .collect(),
            ))
        }
        _ => local.cloned(),
    }
}

pub(super) fn merge_json(
    baseline: &[u8],
    local: &[u8],
    remote: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let Ok(baseline) = serde_json::from_slice::<serde_json::Value>(baseline) else {
        return Ok(None);
    };
    let Ok(local) = serde_json::from_slice::<serde_json::Value>(local) else {
        return Ok(None);
    };
    let Ok(remote) = serde_json::from_slice::<serde_json::Value>(remote) else {
        return Ok(None);
    };
    let Some(merged) = merge_json_value(Some(&baseline), Some(&local), Some(&remote)) else {
        return Ok(None);
    };
    let mut output = serde_json::to_vec_pretty(&merged).map_err(|error| error.to_string())?;
    output.push(b'\n');
    Ok(Some(output))
}

pub(super) fn merge_json_value(
    baseline: Option<&serde_json::Value>,
    local: Option<&serde_json::Value>,
    remote: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    if local == remote {
        return local.cloned();
    }
    if local == baseline {
        return remote.cloned();
    }
    if remote == baseline {
        return local.cloned();
    }
    match (baseline, local, remote) {
        (
            Some(serde_json::Value::Object(baseline)),
            Some(serde_json::Value::Object(local)),
            Some(serde_json::Value::Object(remote)),
        ) => {
            let keys = baseline
                .keys()
                .chain(local.keys())
                .chain(remote.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            Some(serde_json::Value::Object(
                keys.into_iter()
                    .filter_map(|key| {
                        merge_json_value(baseline.get(&key), local.get(&key), remote.get(&key))
                            .map(|value| (key, value))
                    })
                    .collect(),
            ))
        }
        _ => local.cloned(),
    }
}

pub(super) fn conflict_copy_path(
    path: &str,
    occupied: &mut BTreeSet<String>,
) -> Result<String, String> {
    let path = Path::new(path);
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|extension| extension.to_str());
    let label = conflict_copy_label();
    for index in 1..=u16::MAX {
        let suffix = if index == 1 {
            String::new()
        } else {
            format!(" {index}")
        };
        let file_name = match extension {
            Some(extension) => {
                format!("{stem} (Conflicted copy {label}){suffix}.{extension}")
            }
            None => format!("{stem} (Conflicted copy {label}){suffix}"),
        };
        let candidate = parent.join(file_name).to_string_lossy().replace('\\', "/");
        validate_relative_path(&candidate)?;
        if occupied.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    Err("failed to allocate Vault conflict copy".to_string())
}

pub(super) fn conflict_copy_label() -> String {
    let device = Command::new("/bin/hostname")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|device| !device.is_empty())
        .unwrap_or_else(|| "device".to_string());
    let device = device
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let device = device.trim_matches('-');
    let device = if device.is_empty() { "device" } else { device };
    format!(
        "{} {}",
        device,
        chrono::Local::now().format("%Y-%m-%d %H-%M-%S")
    )
}

pub(super) fn validate_file_tree(files: &BTreeMap<String, LocalEntry>) -> Result<(), String> {
    for path in files.keys() {
        let mut parent = Path::new(path).parent();
        while let Some(candidate) = parent {
            if let Some(candidate) = candidate.to_str()
                && files.contains_key(candidate)
            {
                return Err(format!(
                    "Vault has incompatible file and directory changes: {candidate}, {path}"
                ));
            }
            parent = candidate.parent();
        }
    }
    Ok(())
}

pub(super) fn same_entry(left: Option<&LocalEntry>, right: Option<&LocalEntry>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            left.digest == right.digest && left.kind == right.kind && left.mode == right.mode
        }
        (None, None) => true,
        _ => false,
    }
}

pub(super) fn apply_local_entry(
    root: &Path,
    relative: &str,
    entry: Option<&LocalEntry>,
) -> Result<(), String> {
    validate_relative_path(relative)?;
    let path = root.join(relative);
    match entry {
        Some(entry) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            remove_existing_path(&path)?;
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
            remove_existing_path(&path)?;
            prune_empty_parents(root, path.parent());
        }
    }
    Ok(())
}

pub(super) fn remove_existing_path(path: &Path) -> Result<(), String> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path).map_err(|error| error.to_string())
    } else {
        std::fs::remove_file(path).map_err(|error| error.to_string())
    }
}

pub(super) fn prune_empty_parents(root: &Path, mut parent: Option<&Path>) {
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

pub(super) fn write_local_state(root: &Path, repository: &Path) -> Result<(), String> {
    let files = collect_local_files(root)?;
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
    vmux_path::AtomicFile::write(state_path(repository), source.as_bytes())
        .map_err(|error| error.to_string())
}

pub(super) fn local_change_count(root: &Path, repository: &Path) -> Result<u32, String> {
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

pub(super) fn read_local_state(
    repository: &Path,
) -> Result<BTreeMap<String, LocalStateEntry>, String> {
    let source = std::fs::read(state_path(repository)).map_err(|error| error.to_string())?;
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

pub(super) fn baseline_files(repository: &Path) -> Result<BTreeMap<String, LocalEntry>, String> {
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
