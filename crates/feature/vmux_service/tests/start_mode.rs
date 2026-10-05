use std::path::PathBuf;

use vmux_service::DaemonBinary;

#[test]
fn start_mode_depends_on_profile_and_bundle_location() {
    let service_app = PathBuf::from(
        "/Applications/Vmux.app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service",
    );
    let main_app = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");
    let dev_binary = PathBuf::from("/Users/x/repo/target/debug/vmux_service");
    let plain_binary = PathBuf::from("/usr/local/bin/vmux_service");

    for (profile, exe, expected) in [
        ("release", &service_app, true),
        ("release", &main_app, true),
        ("local", &service_app, false),
        ("dev", &main_app, false),
        ("release", &dev_binary, false),
        ("release", &plain_binary, false),
    ] {
        let binary = DaemonBinary::beside(exe);

        assert_eq!(binary.requires_registration(profile), expected);
    }
}
