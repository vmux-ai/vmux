use super::*;
use std::io::Write;

fn fixture_crx(manifest: &str) -> (String, Vec<u8>) {
    let public_key = b"PUBKEY";
    let id = crx::extension_id_from_key(public_key);
    let mut zip_bytes = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.start_file("background.js", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"chrome.runtime.onInstalled.addListener(() => {});")
            .unwrap();
        zip.finish().unwrap();
    }
    let header = [0x12u8, 0x08, 0x0a, 0x06, b'P', b'U', b'B', b'K', b'E', b'Y'];
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"Cr24");
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(&zip_bytes);
    (id, bytes)
}

#[test]
fn installs_source_under_immutable_package_path() {
    let root = tempfile::tempdir().unwrap();
    let (id, bytes) = fixture_crx(
        r#"{
                "manifest_version": 3,
                "name": "Fixture",
                "version": "1.0",
                "background": { "service_worker": "background.js" }
            }"#,
    );

    let entry = install_crx(root.path(), &id, &bytes).unwrap();

    let source = store::source_dir(root.path(), &id, &entry.version);
    assert!(source.join("manifest.json").exists());
    assert!(!root.path().join(&id).exists());
    assert_eq!(entry.source_hash, store::tree_sha256(&source).unwrap());
    assert_eq!(
        entry.public_key_b64,
        Some(base64::engine::general_purpose::STANDARD.encode(b"PUBKEY"))
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(source.join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["background"]["service_worker"], "background.js");
    assert!(entry.installed_for("personal"));
    assert!(!entry.enabled_for("personal"));
    assert_eq!(
        entry.grants_for("personal"),
        store::ExtensionGrants::default()
    );
}

#[test]
fn update_reconciles_grants_and_disables_every_affected_profile() {
    let root = tempfile::tempdir().unwrap();
    let (id, initial) = fixture_crx(
        r#"{
                "manifest_version": 3,
                "name": "Fixture",
                "version": "1.0",
                "permissions": ["storage"],
                "optional_permissions": ["history"],
                "background": { "service_worker": "background.js" }
            }"#,
    );
    install_crx(root.path(), &id, &initial).unwrap();
    store::update_index(root.path(), |index| {
        let entry = index
            .entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .unwrap();
        for profile in ["personal", "work"] {
            entry.profile_enabled.insert(profile.into(), true);
            entry.approved_grants.insert(
                profile.into(),
                store::ExtensionGrants {
                    permissions: vec!["storage".into(), "history".into()],
                    host_permissions: Vec::new(),
                },
            );
        }
    })
    .unwrap();
    let (_, update) = fixture_crx(
        r#"{
                "manifest_version": 3,
                "name": "Fixture",
                "version": "2.0",
                "permissions": ["storage", "bookmarks"],
                "background": { "service_worker": "background.js" }
            }"#,
    );

    let returned = install_crx(root.path(), &id, &update).unwrap();
    let stored = store::Index::load(root.path())
        .unwrap()
        .entries
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap();

    assert_eq!(returned, stored);
    for profile in ["personal", "work"] {
        assert!(!stored.enabled_for(profile));
        assert_eq!(
            stored.grants_for(profile).permissions,
            vec!["storage".to_string()]
        );
    }
}
