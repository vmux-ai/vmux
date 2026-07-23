use std::path::PathBuf;
use vmux_service::registry::{StartMode, start_mode_for_profile};

#[test]
fn start_mode_depends_on_profile_and_bundle_location() {
    let service_app = PathBuf::from(
        "/Applications/Vmux.app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service",
    );
    let main_app = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");
    let dev_binary = PathBuf::from("/Users/x/repo/target/debug/vmux_service");
    let plain_binary = PathBuf::from("/usr/local/bin/vmux_service");

    for (profile, exe, expected) in [
        ("release", &service_app, StartMode::Register),
        ("release", &main_app, StartMode::Register),
        ("local", &service_app, StartMode::SpawnDetached),
        ("dev", &main_app, StartMode::SpawnDetached),
        ("release", &dev_binary, StartMode::SpawnDetached),
        ("release", &plain_binary, StartMode::SpawnDetached),
    ] {
        assert_eq!(start_mode_for_profile(profile, exe), expected);
    }
}
