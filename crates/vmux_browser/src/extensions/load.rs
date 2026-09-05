use vmux_core::extension::manifest;
use vmux_core::extension::store;

use bevy::prelude::Resource;

use super::runtime::{self, PreparedRuntime};
use super::service_worker_cache::ServiceWorkerCache;

#[derive(Resource, Clone, Debug, Default)]
pub struct PreparedExtensions(pub Vec<PreparedRuntime>);

pub fn apply_env() -> Result<Vec<PreparedRuntime>, String> {
    let root = store::root();
    let runtime_store = runtime_store_root();
    let profile = vmux_core::profile::active_profile_name();
    let mut idx = store::Index::load(&root)?;
    let migrating = idx.requires_save();
    let mut index_changed = migrating;
    if migrating {
        migrate_index_permissions(&root, &mut idx)?;
    }
    let (prepared, preparation_changed) =
        prepare_enabled_entries(&profile, &mut idx.entries, |entry| {
            runtime::prepare_runtime_in(&root, &runtime_store, &profile, entry)
        })?;
    index_changed |= preparation_changed;
    if index_changed {
        idx.save(&root)?;
    }
    ServiceWorkerCache::of(&vmux_core::profile::profile_dir()).reconcile(&prepared)?;
    let dirs = prepared
        .iter()
        .map(|item| item.dir.to_string_lossy())
        .collect::<Vec<_>>();
    if dirs.is_empty() {
        unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
    } else {
        unsafe { std::env::set_var("VMUX_LOAD_EXTENSIONS", dirs.join(",")) };
    }
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let loaded_path = loaded_path(&root, &profile);
    if let Some(parent) = loaded_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(loaded_path, idx.enabled_ids_for(&profile).join("\n"))
        .map_err(|error| error.to_string())?;
    Ok(prepared)
}

fn prepare_enabled_entries(
    profile: &str,
    entries: &mut [store::ExtEntry],
    mut prepare: impl FnMut(&store::ExtEntry) -> Result<PreparedRuntime, runtime::PrepareRuntimeError>,
) -> Result<(Vec<PreparedRuntime>, bool), String> {
    let mut prepared = Vec::new();
    let mut changed = false;
    for entry in entries
        .iter_mut()
        .filter(|entry| entry.enabled_for(profile))
    {
        match prepare(entry) {
            Ok(item) => {
                if entry.source_hash.is_empty() {
                    entry.source_hash.clone_from(&item.source_hash);
                    changed = true;
                }
                prepared.push(item);
            }
            Err(runtime::PrepareRuntimeError::Corrupt(error)) => {
                bevy::log::error!(
                    extension_id = %entry.id,
                    %profile,
                    %error,
                    "disabling extension after preparation failure"
                );
                entry.profile_enabled.insert(profile.to_string(), false);
                changed = true;
            }
            Err(runtime::PrepareRuntimeError::Infrastructure(error)) => {
                return Err(format!("failed to prepare extension {}: {error}", entry.id));
            }
        }
    }
    Ok((prepared, changed))
}

fn runtime_store_root() -> std::path::PathBuf {
    vmux_core::profile::shared_data_dir().join("extensions")
}

pub fn loaded_ids() -> Vec<String> {
    let root = store::root();
    let profile = vmux_core::profile::active_profile_name();
    let profile_path = loaded_path(&root, &profile);
    std::fs::read_to_string(profile_path)
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                std::fs::read_to_string(root.join("loaded.txt"))
            } else {
                Err(error)
            }
        })
        .ok()
        .map(|s| {
            s.lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn loaded_path(root: &std::path::Path, profile: &str) -> std::path::PathBuf {
    root.join("loaded").join(format!("{profile}.txt"))
}

fn migrate_index_permissions(
    root: &std::path::Path,
    index: &mut store::Index,
) -> Result<(), String> {
    for entry in &mut index.entries {
        let expected = store::source_dir(root, &entry.id, &entry.version);
        let source = if expected.exists() {
            expected
        } else {
            store::migrate_legacy_package(root, entry)?
        };
        let text = std::fs::read_to_string(source.join("manifest.json"))
            .map_err(|error| error.to_string())?;
        let parsed = manifest::parse(&text)?;
        entry.permissions = parsed.permissions;
        entry.optional_permissions = parsed.optional_permissions;
        entry.host_permissions = parsed.host_permissions;
        entry.optional_host_permissions = parsed.optional_host_permissions;
        for profile in entry
            .profile_enabled
            .iter()
            .filter_map(|(profile, enabled)| enabled.then_some(profile.clone()))
            .collect::<Vec<_>>()
        {
            entry.approved_grants.insert(
                profile,
                store::ExtensionGrants {
                    permissions: entry.permissions.clone(),
                    host_permissions: entry.host_permissions.clone(),
                },
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_entry(id: &str) -> store::ExtEntry {
        let mut profile_enabled = std::collections::BTreeMap::new();
        profile_enabled.insert("personal".to_string(), true);
        store::ExtEntry {
            id: id.to_string(),
            name: id.to_string(),
            version: "1".to_string(),
            popup: None,
            icon: None,
            enabled: false,
            profile_enabled,
            permissions: Vec::new(),
            optional_permissions: Vec::new(),
            host_permissions: Vec::new(),
            optional_host_permissions: Vec::new(),
            approved_grants: std::collections::BTreeMap::new(),
            source_hash: "source-hash".to_string(),
            public_key_b64: None,
        }
    }

    #[test]
    fn preparation_failure_disables_only_the_broken_extension() {
        let mut entries = vec![enabled_entry("broken"), enabled_entry("working")];

        let (prepared, changed) = prepare_enabled_entries("personal", &mut entries, |entry| {
            if entry.id == "broken" {
                Err(runtime::PrepareRuntimeError::Corrupt(
                    "source hash mismatch".to_string(),
                ))
            } else {
                Ok(PreparedRuntime::fixture(&entry.id, "runtime-hash"))
            }
        })
        .unwrap();

        assert!(changed);
        assert!(!entries[0].enabled_for("personal"));
        assert!(entries[1].enabled_for("personal"));
        assert_eq!(
            prepared
                .iter()
                .map(|runtime| runtime.extension_id.as_str())
                .collect::<Vec<_>>(),
            ["working"]
        );
    }

    #[test]
    fn infrastructure_failure_keeps_extensions_enabled() {
        let mut entries = vec![enabled_entry("broken"), enabled_entry("working")];

        let error = prepare_enabled_entries("personal", &mut entries, |entry| {
            if entry.id == "broken" {
                Err(runtime::PrepareRuntimeError::Infrastructure(
                    "read-only runtime store".to_string(),
                ))
            } else {
                Ok(PreparedRuntime::fixture(&entry.id, "runtime-hash"))
            }
        })
        .unwrap_err();

        assert_eq!(
            error,
            "failed to prepare extension broken: read-only runtime store"
        );
        assert!(entries.iter().all(|entry| entry.enabled_for("personal")));
    }

    #[test]
    fn migration_populates_every_entry_and_enabled_profile() {
        let root = tempfile::tempdir().unwrap();
        let ids = [
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ];
        for (id, permission) in ids.iter().zip(["storage", "bookmarks"]) {
            let source = store::source_dir(root.path(), id, "1");
            std::fs::create_dir_all(&source).unwrap();
            std::fs::write(
                source.join("manifest.json"),
                serde_json::json!({
                    "manifest_version": 3,
                    "name": id,
                    "version": "1",
                    "permissions": [permission],
                })
                .to_string(),
            )
            .unwrap();
        }
        std::fs::write(
            root.path().join("index.json"),
            serde_json::json!({
                "version": 2,
                "entries": [
                    {
                        "id": ids[0], "name": "one", "version": "1", "popup": null,
                        "icon": null, "enabled": false,
                        "profile_enabled": {"personal": true}
                    },
                    {
                        "id": ids[1], "name": "two", "version": "1", "popup": null,
                        "icon": null, "enabled": false,
                        "profile_enabled": {"work": true}
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();
        let mut index = store::Index::load(root.path()).unwrap();

        migrate_index_permissions(root.path(), &mut index).unwrap();

        assert_eq!(index.entries[0].permissions, ["storage"]);
        assert_eq!(
            index.entries[0].grants_for("personal").permissions,
            ["storage"]
        );
        assert_eq!(index.entries[1].permissions, ["bookmarks"]);
        assert_eq!(
            index.entries[1].grants_for("work").permissions,
            ["bookmarks"]
        );
    }
}
