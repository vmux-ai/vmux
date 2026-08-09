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
            Ok(prepared_runtime(&entry.id))
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
            Ok(prepared_runtime(&entry.id))
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
