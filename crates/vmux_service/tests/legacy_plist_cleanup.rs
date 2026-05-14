use std::fs;
use vmux_service::legacy_plist_cleanup;

#[test]
fn finds_and_lists_legacy_plists() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("ai.vmux.service.plist"), "<plist/>").unwrap();
    fs::write(dir.path().join("ai.vmux.service.dev.plist"), "<plist/>").unwrap();
    fs::write(dir.path().join("ai.vmux.service.abc1234.plist"), "<plist/>").unwrap();
    fs::write(dir.path().join("com.unrelated.app.plist"), "<plist/>").unwrap();

    let found = legacy_plist_cleanup::find_legacy_plists_in(dir.path()).unwrap();
    assert_eq!(
        found.len(),
        3,
        "should find 3 vmux plists, ignoring unrelated: {found:?}"
    );
}

#[test]
fn extracts_label_from_filename() {
    assert_eq!(
        legacy_plist_cleanup::label_from_filename("ai.vmux.service.dev.plist"),
        Some("ai.vmux.service.dev")
    );
    assert_eq!(
        legacy_plist_cleanup::label_from_filename("ai.vmux.service.plist"),
        Some("ai.vmux.service")
    );
    assert_eq!(
        legacy_plist_cleanup::label_from_filename("com.other.plist"),
        None
    );
}

#[test]
fn cleanup_removes_files() {
    let dir = tempfile::tempdir().unwrap();
    let plist = dir.path().join("ai.vmux.service.dev.plist");
    fs::write(&plist, "<plist/>").unwrap();
    assert!(plist.exists());

    legacy_plist_cleanup::remove_plist_files(std::slice::from_ref(&plist)).unwrap();
    assert!(!plist.exists());
}

#[test]
fn cleanup_is_idempotent_when_no_files_present() {
    let dir = tempfile::tempdir().unwrap();
    let found = legacy_plist_cleanup::find_legacy_plists_in(dir.path()).unwrap();
    assert!(found.is_empty());
}
