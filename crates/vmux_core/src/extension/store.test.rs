use super::*;

fn entry(id: &str, enabled: bool) -> ExtEntry {
    let mut profile_enabled = BTreeMap::new();
    profile_enabled.insert(LEGACY_PROFILE.into(), enabled);
    ExtEntry {
        id: id.into(),
        name: id.into(),
        version: "1".into(),
        popup: None,
        icon: None,
        enabled: false,
        profile_enabled,
        permissions: Vec::new(),
        optional_permissions: Vec::new(),
        host_permissions: Vec::new(),
        optional_host_permissions: Vec::new(),
        approved_grants: BTreeMap::new(),
        source_hash: String::new(),
        public_key_b64: None,
    }
}

#[test]
fn index_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let mut idx = Index::default();
    idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
    idx.save(dir.path()).unwrap();
    let loaded = Index::load(dir.path()).unwrap();
    assert_eq!(loaded.entries.len(), 1);
    assert!(loaded.entries[0].enabled_for("personal"));
}

#[test]
fn upsert_replaces_existing() {
    let mut idx = Index::default();
    idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
    idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", false));
    assert_eq!(idx.entries.len(), 1);
    assert!(!idx.entries[0].enabled_for("personal"));
}

#[test]
fn enabled_dirs_reflects_profile_toggle() {
    let root = tempfile::tempdir().unwrap();
    let mut idx = Index::default();
    idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
    idx.upsert(entry("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false));
    idx.entries[0].profile_enabled.insert("work".into(), false);
    idx.entries[1].profile_enabled.insert("work".into(), false);
    assert_eq!(
        idx.set_enabled_for("work", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", true, true),
        EnableForProfileResult::Updated
    );
    let dirs = idx.enabled_dirs_for(root.path(), "work");
    assert_eq!(dirs.len(), 1);
    assert_eq!(
        dirs[0],
        source_dir(root.path(), "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "1")
    );
}

#[test]
fn uninstall_rejects_non_extension_id() {
    let root = tempfile::tempdir().unwrap();
    assert!(uninstall(root.path(), "../evil").is_err());
    assert!(uninstall(root.path(), "/etc/passwd").is_err());
    assert!(uninstall(root.path(), "short").is_err());
}

#[test]
fn uninstall_removes_packages_and_profile_runtimes() {
    let root = tempfile::tempdir().unwrap();
    let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let package = packages_root(root.path()).join(id);
    let personal = runtime_profile_dir(root.path(), "personal", id);
    let work = runtime_profile_dir(root.path(), "work", id);
    std::fs::create_dir_all(&package).unwrap();
    std::fs::create_dir_all(&personal).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    let mut index = Index::default();
    index.upsert(entry(id, true));
    index.save(root.path()).unwrap();

    uninstall(root.path(), id).unwrap();

    assert!(!package.exists());
    assert!(!personal.exists());
    assert!(!work.exists());
    assert!(Index::load(root.path()).unwrap().entries.is_empty());
}

#[test]
fn dirty_when_enabled_set_differs_from_loaded() {
    let mut idx = Index::default();
    idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
    assert!(idx.is_dirty_for("personal", &[]));
    assert!(!idx.is_dirty_for(
        "personal",
        &["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()]
    ));
}

#[test]
fn profile_overrides_preserve_legacy_default() {
    let mut item = entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true);
    item.profile_enabled.insert("work".into(), false);

    assert!(item.enabled_for("personal"));
    assert!(!item.enabled_for("work"));
    assert!(!item.enabled_for("new-profile"));
}

#[test]
fn approved_grants_do_not_cover_permission_expansion() {
    let grants = ExtensionGrants {
        permissions: vec!["storage".into()],
        host_permissions: vec!["https://example.com/*".into()],
    };

    assert!(grants.covers(&["storage".into()], &["https://example.com/*".into()]));
    assert!(!grants.covers(
        &["storage".into(), "history".into()],
        &["https://example.com/*".into()]
    ));
}

#[test]
fn enabling_requires_permission_approval() {
    let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let mut item = entry(id, false);
    item.permissions = vec!["storage".into()];
    let mut idx = Index::default();
    idx.upsert(item);

    assert_eq!(
        idx.set_enabled_for("personal", id, true, false),
        EnableForProfileResult::NeedsApproval
    );
    assert!(!idx.entries[0].enabled_for("personal"));
    assert_eq!(
        idx.set_enabled_for("personal", id, true, true),
        EnableForProfileResult::Updated
    );
    assert!(idx.entries[0].enabled_for("personal"));
    assert_eq!(
        idx.entries[0].grants_for("personal").permissions,
        vec!["storage".to_string()]
    );
}

#[test]
fn profile_uninstall_preserves_shared_package_until_unused() {
    let root = tempfile::tempdir().unwrap();
    let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let package = packages_root(root.path()).join(id);
    std::fs::create_dir_all(&package).unwrap();
    let mut item = entry(id, true);
    item.profile_enabled.insert("work".into(), true);
    let mut index = Index::default();
    index.upsert(item);
    index.save(root.path()).unwrap();

    uninstall_for_profile(root.path(), "personal", id).unwrap();

    let index = Index::load(root.path()).unwrap();
    assert_eq!(index.entries.len(), 1);
    assert!(!index.entries[0].installed_for("personal"));
    assert!(index.entries[0].installed_for("work"));
    assert!(package.exists());

    uninstall_for_profile(root.path(), "work", id).unwrap();

    assert!(Index::load(root.path()).unwrap().entries.is_empty());
    assert!(!package.exists());
}

#[test]
fn legacy_global_enablement_migrates_only_to_personal_profile() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("index.json"),
        serde_json::json!({
            "entries": [{
                "id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "name": "legacy",
                "version": "1",
                "popup": null,
                "icon": null,
                "enabled": true
            }]
        })
        .to_string(),
    )
    .unwrap();

    let index = Index::load(root.path()).unwrap();
    let entry = &index.entries[0];

    assert!(index.requires_save());
    assert!(entry.enabled_for("personal"));
    assert!(!entry.enabled_for("work"));
    assert!(!entry.enabled);
}

#[test]
fn migrates_legacy_package_without_generated_files() {
    let root = tempfile::tempdir().unwrap();
    let entry = entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true);
    let legacy = root.path().join(&entry.id);
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(
        legacy.join("manifest.json"),
        serde_json::json!({
            "manifest_version": 3,
            "name": "test",
            "version": entry.version,
            "background": { "service_worker": "vmux_sw_deadbeef.js" },
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(legacy.join("background.js"), "original").unwrap();
    std::fs::write(legacy.join("vmux_patch.js"), "patch").unwrap();
    std::fs::write(legacy.join("vmux_sw_deadbeef.js"), "loader").unwrap();
    std::fs::write(
        legacy.join("vmux_shim.json"),
        serde_json::json!({
            "original": "background.js",
            "loader": "vmux_sw_deadbeef.js",
        })
        .to_string(),
    )
    .unwrap();

    let migrated = migrate_legacy_package(root.path(), &entry).unwrap();
    assert_eq!(migrated, source_dir(root.path(), &entry.id, &entry.version));
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(migrated.join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["background"]["service_worker"], "background.js");
    assert!(!migrated.join("vmux_patch.js").exists());
    assert!(!migrated.join("vmux_shim.json").exists());
    assert_eq!(tree_sha256(&migrated).unwrap().len(), 64);
    assert!(legacy.join("vmux_shim.json").exists());
}
