use std::path::Path;

use crate::filesystem::FileStore;
use crate::{Catalog, Evidence, PreparedEvidence, driver, snapshot};
use bevy::prelude::Component;

use super::runtime::{self, PreparedRuntime};
use super::service_worker_cache::ServiceWorkerCache;

#[derive(Component, Clone, Debug, Default)]
pub struct PreparedExtensions(pub Vec<PreparedRuntime>);

impl PreparedExtensions {
    pub(crate) fn snapshot(
        &self,
        index: &snapshot::Index,
        profile: &str,
        loaded: &[String],
    ) -> vmux_api::extension::ExtensionsUiState {
        let mut state = Catalog::project_ui_state(index, profile, loaded);
        for entry in &index.entries {
            if !entry.enabled_for(profile) {
                continue;
            }
            let Some(runtime) = self
                .0
                .iter()
                .find(|runtime| runtime.extension_id == entry.id)
            else {
                state.pending = true;
                continue;
            };
            let grants = entry.grants_for(profile);
            if runtime.source_hash != entry.source_hash
                || runtime.granted_permissions != grants.permissions
                || runtime.granted_host_permissions != grants.host_permissions
            {
                state.pending = true;
            }
        }
        state
    }

    pub(super) fn prepare(index: &snapshot::Index) -> Result<(Self, PreparedEvidence), String> {
        let source_root = vmux_ecs::profile::ProfilePaths::current().extensions();
        let runtime_root = Self::runtime_store_root();
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let (prepared, evidence) =
            Self::prepare_enabled_entries(&profile, &index.entries, |entry| {
                PreparedRuntime::prepare(&source_root, &runtime_root, &profile, entry)
            })?;
        let profile_dir = vmux_ecs::profile::ProfilePaths::current().profile();
        ServiceWorkerCache::from(profile_dir.as_path()).reconcile(&prepared)?;
        let mut directories = Vec::new();
        let mut loaded = Vec::new();
        for runtime in &prepared {
            directories.push(runtime.dir.to_string_lossy().into_owned());
            loaded.push(runtime.extension_id.clone());
        }
        if directories.is_empty() {
            unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
        } else {
            unsafe { std::env::set_var("VMUX_LOAD_EXTENSIONS", directories.join(",")) };
        }
        FileStore::write_loaded_ids(&source_root, &profile, &loaded)?;
        Ok((Self(prepared), evidence))
    }

    fn prepare_enabled_entries(
        profile: &str,
        entries: &[snapshot::ExtEntry],
        mut prepare: impl FnMut(
            &snapshot::ExtEntry,
        ) -> Result<PreparedRuntime, runtime::PrepareRuntimeError>,
    ) -> Result<(Vec<PreparedRuntime>, PreparedEvidence), String> {
        let mut prepared = Vec::new();
        let mut evidence = Vec::new();
        for entry in entries {
            if !entry.enabled_for(profile) {
                continue;
            }
            match prepare(entry) {
                Ok(runtime) => {
                    if entry.source_hash.is_empty() {
                        evidence.push(Evidence {
                            id: entry.id.clone(),
                            profile: profile.into(),
                            source_hash: Some(runtime.source_hash.clone()),
                            ..Default::default()
                        });
                    }
                    prepared.push(runtime);
                }
                Err(runtime::PrepareRuntimeError::Corrupt(error)) => {
                    evidence.push(Evidence {
                        id: entry.id.clone(),
                        profile: profile.into(),
                        error: Some(error),
                        ..Default::default()
                    });
                }
                Err(runtime::PrepareRuntimeError::Infrastructure(error)) => {
                    return Err(format!("failed to prepare extension {}: {error}", entry.id));
                }
            }
        }
        Ok((prepared, PreparedEvidence(evidence)))
    }

    fn runtime_store_root() -> std::path::PathBuf {
        vmux_ecs::profile::ProfilePaths::current()
            .shared_data()
            .join("extensions")
    }

    pub(super) fn migrate_catalog_permissions(
        root: &Path,
        index: &snapshot::Index,
    ) -> Result<PreparedEvidence, String> {
        let mut evidence = Vec::new();
        for entry in &index.entries {
            let expected = FileStore::source_dir(root, &entry.id, &entry.version);
            let source = if expected.exists() {
                expected
            } else {
                FileStore::migrate_legacy_package(
                    root,
                    &entry.id,
                    &entry.version,
                    &entry.source_hash,
                )?
            };
            let text = std::fs::read_to_string(source.join("manifest.json"))
                .map_err(|error| error.to_string())?;
            let parsed = driver::ExtensionManifest::parse(&text)?;
            evidence.push(Evidence {
                id: entry.id.clone(),
                declared_permissions: Some(parsed.permissions),
                optional_permissions: Some(parsed.optional_permissions),
                host_permissions: Some(parsed.host_permissions),
                optional_host_permissions: Some(parsed.optional_host_permissions),
                grant_enabled_profiles: true,
                ..Default::default()
            });
        }
        Ok(PreparedEvidence(evidence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Catalog, CorePlugin, ExtensionSchedule, Root, Seed};
    use bevy::ecs::system::SystemState;
    use bevy::prelude::App;

    #[test]
    fn preparation_failure_disables_only_the_broken_extension() {
        let mut entries = Vec::new();
        for id in ["broken", "working"] {
            let mut profile_enabled = std::collections::BTreeMap::new();
            profile_enabled.insert("personal".to_string(), true);
            entries.push(snapshot::ExtEntry {
                id: id.to_string(),
                name: id.to_string(),
                version: "1".to_string(),
                profile_enabled,
                source_hash: "source-hash".to_string(),
                ..Default::default()
            });
        }

        let (prepared, evidence) =
            PreparedExtensions::prepare_enabled_entries("personal", &entries, |entry| {
                if entry.id == "broken" {
                    Err(runtime::PrepareRuntimeError::Corrupt(
                        "source hash mismatch".to_string(),
                    ))
                } else {
                    Ok(PreparedRuntime::fixture(&entry.id, "runtime-hash"))
                }
            })
            .unwrap();

        assert_eq!(evidence.0.len(), 1);
        assert!(entries[0].enabled_for("personal"));
        assert_eq!(evidence.0[0].id, "broken");
        assert!(evidence.0[0].error.is_some());
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
    fn same_identity_updates_and_grant_changes_require_a_prepared_runtime_refresh() {
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let mut index = snapshot::Index::default();
        let mut profile_enabled = std::collections::BTreeMap::new();
        profile_enabled.insert("personal".to_string(), true);
        index.entries.push(snapshot::ExtEntry {
            id: id.to_string(),
            name: id.to_string(),
            version: "1".to_string(),
            profile_enabled,
            source_hash: "source-hash".to_string(),
            ..Default::default()
        });
        let prepared = PreparedExtensions(vec![PreparedRuntime::fixture(id, "runtime-hash")]);
        let loaded = vec![id.to_string()];
        assert!(!prepared.snapshot(&index, "personal", &loaded).pending);
        index.entries[0].source_hash = "updated-source-hash".into();
        assert!(prepared.snapshot(&index, "personal", &loaded).pending);
        index.entries[0].source_hash = "source-hash".into();
        index.entries[0].approved_grants.insert(
            "personal".into(),
            snapshot::ExtensionGrants {
                permissions: vec!["storage".into()],
                host_permissions: Vec::new(),
            },
        );
        assert!(prepared.snapshot(&index, "personal", &loaded).pending);
        let mut updated = PreparedRuntime::fixture(id, "runtime-hash");
        updated.granted_permissions = vec!["storage".into()];
        assert!(
            !PreparedExtensions(vec![updated])
                .snapshot(&index, "personal", &loaded)
                .pending
        );
    }

    #[test]
    fn infrastructure_failure_keeps_extensions_enabled() {
        let mut entries = Vec::new();
        for id in ["broken", "working"] {
            let mut profile_enabled = std::collections::BTreeMap::new();
            profile_enabled.insert("personal".to_string(), true);
            entries.push(snapshot::ExtEntry {
                id: id.to_string(),
                name: id.to_string(),
                version: "1".to_string(),
                profile_enabled,
                source_hash: "source-hash".to_string(),
                ..Default::default()
            });
        }

        let error = PreparedExtensions::prepare_enabled_entries("personal", &entries, |entry| {
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
            let source = FileStore::source_dir(root.path(), id, "1");
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
        let index = FileStore::read(root.path()).unwrap();
        let mut app = App::new();
        app.add_plugins(CorePlugin);
        let owner = app
            .world_mut()
            .spawn((Root(root.path().to_path_buf()), Seed(Ok(index))))
            .id();
        app.world_mut().run_schedule(ExtensionSchedule);
        let mut catalog = SystemState::<Catalog>::new(app.world_mut());
        let initial = catalog.get(app.world()).unwrap().snapshot(owner).unwrap();
        let evidence =
            PreparedExtensions::migrate_catalog_permissions(root.path(), &initial).unwrap();
        app.world_mut().entity_mut(owner).insert(evidence);
        app.world_mut().run_schedule(ExtensionSchedule);
        let migrated = catalog.get(app.world()).unwrap().snapshot(owner).unwrap();

        assert_eq!(migrated.entries[0].permissions, ["storage"]);
        assert_eq!(
            migrated.entries[0].grants_for("personal").permissions,
            ["storage"]
        );
        assert_eq!(migrated.entries[1].permissions, ["bookmarks"]);
        assert_eq!(
            migrated.entries[1].grants_for("work").permissions,
            ["bookmarks"]
        );
    }
}
