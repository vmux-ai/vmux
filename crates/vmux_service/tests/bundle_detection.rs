use std::path::PathBuf;
use vmux_service::bundle;

#[test]
fn detects_bundled_when_exe_inside_app_macos() {
    let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");
    assert!(bundle::is_bundled_path(&exe));
}

#[test]
fn detects_not_bundled_when_target_debug() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_desktop");
    assert!(!bundle::is_bundled_path(&exe));
}

#[test]
fn bundle_root_resolves_app_path() {
    let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");
    assert_eq!(
        bundle::bundle_root_for(&exe).unwrap(),
        PathBuf::from("/Applications/Vmux.app")
    );
}

#[test]
fn bundle_root_none_when_not_bundled() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_desktop");
    assert!(bundle::bundle_root_for(&exe).is_none());
}
