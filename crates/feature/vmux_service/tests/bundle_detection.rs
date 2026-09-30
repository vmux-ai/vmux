use std::path::PathBuf;
use vmux_service::bundle::AppBundle;

#[test]
fn detects_bundled_when_exe_inside_app_macos() {
    let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");
    assert!(AppBundle::try_from(exe.as_path()).is_ok());
}

#[test]
fn detects_not_bundled_when_target_debug() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_desktop");
    assert!(AppBundle::try_from(exe.as_path()).is_err());
}

#[test]
fn bundle_root_resolves_app_path() {
    let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");
    assert_eq!(
        AppBundle::try_from(exe.as_path()).unwrap().path(),
        PathBuf::from("/Applications/Vmux.app").as_path()
    );
}

#[test]
fn bundle_root_none_when_not_bundled() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_desktop");
    assert!(AppBundle::try_from(exe.as_path()).is_err());
}
