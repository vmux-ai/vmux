use vmux_core::extension::manifest;
use vmux_core::extension::store;

use bevy::prelude::Resource;

use super::runtime::{self, PreparedRuntime};

#[derive(Resource, Clone, Debug, Default)]
pub struct PreparedExtensions(pub Vec<PreparedRuntime>);

pub fn apply_env() -> Result<Vec<PreparedRuntime>, String> {
    let root = store::root();
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
        let item = runtime::prepare_runtime(&root, &profile, entry)?;
        if entry.source_hash.is_empty() {
            entry.source_hash.clone_from(&item.source_hash);
            index_changed = true;
        }
        prepared.push(item);
    }
    if index_changed {
        idx.save(&root)?;
    }
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
