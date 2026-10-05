use std::fs;
use vmux_service::cleanup::LegacyRegistrations;

#[test]
fn finds_and_lists_legacy_plists() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("ai.vmux.service.plist"), "<plist/>").unwrap();
    fs::write(dir.path().join("ai.vmux.service.dev.plist"), "<plist/>").unwrap();
    fs::write(dir.path().join("ai.vmux.service.abc1234.plist"), "<plist/>").unwrap();
    fs::write(dir.path().join("com.unrelated.app.plist"), "<plist/>").unwrap();

    let found = LegacyRegistrations::in_directory(dir.path()).unwrap();
    assert_eq!(
        found.paths().len(),
        3,
        "should find 3 vmux plists, ignoring unrelated: {found:?}"
    );
}

#[test]
fn extracts_label_from_filename() {
    assert_eq!(
        LegacyRegistrations::label("ai.vmux.service.dev.plist"),
        Some("ai.vmux.service.dev")
    );
    assert_eq!(
        LegacyRegistrations::label("ai.vmux.service.plist"),
        Some("ai.vmux.service")
    );
    assert_eq!(LegacyRegistrations::label("com.other.plist"), None);
}

#[test]
fn cleanup_removes_files() {
    let dir = tempfile::tempdir().unwrap();
    let plist = dir.path().join("ai.vmux.service.dev.plist");
    fs::write(&plist, "<plist/>").unwrap();
    assert!(plist.exists());

    LegacyRegistrations::in_directory(dir.path())
        .unwrap()
        .remove_files()
        .unwrap();
    assert!(!plist.exists());
}

#[test]
fn cleanup_is_idempotent_when_no_files_present() {
    let dir = tempfile::tempdir().unwrap();
    let found = LegacyRegistrations::in_directory(dir.path()).unwrap();
    assert!(found.paths().is_empty());
}
