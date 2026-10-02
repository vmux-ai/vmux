use crate::{manifest, store};

use bevy::prelude::Component;

use super::runtime::{self, PreparedRuntime};
use super::service_worker_cache::ServiceWorkerCache;

#[derive(Component, Clone, Debug, Default)]
pub struct PreparedExtensions(pub Vec<PreparedRuntime>);

impl PreparedExtensions {
    pub fn load() -> Result<Self, String> {
        let store = store::ExtensionStore::current();
        let runtime_store = store::ExtensionStore::at(Self::runtime_store_root());
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let mut index = store.load_index()?;
        let migrating = index.requires_save();
        let mut index_changed = migrating;
        if migrating {
            Self::migrate_index_permissions(&store, &mut index)?;
        }
        let (prepared, preparation_changed) =
            Self::prepare_enabled_entries(&profile, &mut index.entries, |entry| {
                PreparedRuntime::prepare(&store, &runtime_store, &profile, entry)
            })?;
        index_changed |= preparation_changed;
        if index_changed {
            store.save_index(&index)?;
        }
        let profile_dir = vmux_ecs::profile::ProfilePaths::current().profile();
        ServiceWorkerCache::from(profile_dir.as_path()).reconcile(&prepared)?;
        let directories = prepared
            .iter()
            .map(|item| item.dir.to_string_lossy())
            .collect::<Vec<_>>();
        if directories.is_empty() {
            unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
        } else {
            unsafe { std::env::set_var("VMUX_LOAD_EXTENSIONS", directories.join(",")) };
        }
        store.save_loaded_ids(&profile, &index.enabled_ids_for(&profile))?;
        Ok(Self(prepared))
    }

    fn prepare_enabled_entries(
        profile: &str,
        entries: &mut [store::ExtEntry],
        mut prepare: impl FnMut(
            &store::ExtEntry,
        ) -> Result<PreparedRuntime, runtime::PrepareRuntimeError>,
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
        vmux_ecs::profile::ProfilePaths::current()
            .shared_data()
            .join("extensions")
    }

    fn migrate_index_permissions(
        store: &store::ExtensionStore,
        index: &mut store::Index,
    ) -> Result<(), String> {
        for entry in &mut index.entries {
            let expected = store.source_dir(&entry.id, &entry.version);
            let source = if expected.exists() {
                expected
            } else {
                store.migrate_legacy_package(entry)?
            };
            let text = std::fs::read_to_string(source.join("manifest.json"))
                .map_err(|error| error.to_string())?;
            let parsed = manifest::ExtensionManifest::parse(&text)?;
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
            profile_pinned: std::collections::BTreeMap::new(),
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

        let (prepared, changed) =
            PreparedExtensions::prepare_enabled_entries("personal", &mut entries, |entry| {
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

        let error =
            PreparedExtensions::prepare_enabled_entries("personal", &mut entries, |entry| {
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
        let store = store::ExtensionStore::at(root.path());
        let ids = [
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ];
        for (id, permission) in ids.iter().zip(["storage", "bookmarks"]) {
            let source = store.source_dir(id, "1");
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
        let mut index = store.load_index().unwrap();

        PreparedExtensions::migrate_index_permissions(&store, &mut index).unwrap();

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
