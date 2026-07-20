use vmux_core::extension::manifest;
use vmux_core::extension::store;

use bevy::prelude::Resource;
use std::path::Path;

use super::runtime::{self, PreparedRuntime};

const STABLE_RUNTIME_MARKER: &str = ".stable-runtime-v1";

#[derive(Resource, Clone, Debug, Default)]
pub struct PreparedExtensions(pub Vec<PreparedRuntime>);

pub fn apply_env() -> Result<Vec<PreparedRuntime>, String> {
    let root = store::root();
    let runtime_store = runtime_store_root();
    let profile = vmux_core::profile::active_profile_name();
    let mut idx = store::Index::load(&root)?;
    let migrating = idx.requires_save();
    let mut prepared = Vec::new();
    let mut index_changed = migrating;
    if migrating {
        migrate_index_permissions(&root, &mut idx)?;
    }
    for entry in idx
        .entries
        .iter_mut()
        .filter(|entry| entry.enabled_for(&profile))
    {
        let item = runtime::prepare_runtime_in(&root, &runtime_store, &profile, entry)?;
        if entry.source_hash.is_empty() {
            entry.source_hash.clone_from(&item.source_hash);
            index_changed = true;
        }
        prepared.push(item);
    }
    if index_changed {
        idx.save(&root)?;
    }
    migrate_service_worker_cache(&vmux_core::profile::profile_dir(), &prepared)?;
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

fn migrate_service_worker_cache(
    cef_profile: &Path,
    prepared: &[PreparedRuntime],
) -> Result<(), String> {
    if prepared.is_empty() {
        return Ok(());
    }
    let marker = cef_profile.join("Default").join(STABLE_RUNTIME_MARKER);
    if marker.exists() {
        return Ok(());
    }
    let service_workers = cef_profile.join("Default").join("Service Worker");
    if service_workers.exists() {
        let stale = service_workers.with_file_name(format!(
            "Service Worker.vmux-stale-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::rename(&service_workers, &stale).map_err(|error| error.to_string())?;
        let _ = std::thread::Builder::new()
            .name("extension-cache-cleanup".into())
            .spawn(move || {
                let _ = std::fs::remove_dir_all(stale);
            });
    }
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        marker,
        prepared
            .iter()
            .map(|runtime| runtime.extension_id.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .map_err(|error| error.to_string())
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

    fn prepared_runtime(id: &str) -> PreparedRuntime {
        PreparedRuntime {
            extension_id: id.into(),
            dir: "current".into(),
            runtime_hash: "runtime-hash".into(),
            source_hash: "source-hash".into(),
            permissions: Vec::new(),
            optional_permissions: Vec::new(),
            host_permissions: Vec::new(),
            optional_host_permissions: Vec::new(),
            granted_permissions: Vec::new(),
            granted_host_permissions: Vec::new(),
        }
    }

    #[test]
    fn stable_runtime_migration_clears_service_worker_cache_once() {
        let cef_profile = tempfile::tempdir().unwrap();
        let service_workers = cef_profile.path().join("Default/Service Worker");
        std::fs::create_dir_all(&service_workers).unwrap();
        std::fs::write(service_workers.join("registration"), "stale").unwrap();
        let prepared = [prepared_runtime("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")];

        migrate_service_worker_cache(cef_profile.path(), &prepared).unwrap();

        assert!(!service_workers.exists());
        let marker = cef_profile
            .path()
            .join("Default")
            .join(STABLE_RUNTIME_MARKER);
        assert_eq!(
            std::fs::read_to_string(marker).unwrap(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );

        std::fs::create_dir_all(&service_workers).unwrap();
        std::fs::write(service_workers.join("registration"), "current").unwrap();
        migrate_service_worker_cache(cef_profile.path(), &prepared).unwrap();
        assert!(service_workers.join("registration").exists());
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
