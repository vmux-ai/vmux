use std::path::PathBuf;

use vmux_service::DaemonBinary;

#[test]
fn start_mode_depends_on_profile_bundle_and_inherited_environment() {
    let variables = ["VMUX_DATA_DIR", "VMUX_TEST", "VMUX_PROFILE"];
    let previous = variables.map(|name| std::env::var_os(name));
    for name in variables {
        unsafe { std::env::remove_var(name) };
    }
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
    let bundled = DaemonBinary::beside(&main_app);
    for (name, value) in [
        ("VMUX_DATA_DIR", "/tmp/copied workspace"),
        ("VMUX_TEST", "1"),
        ("VMUX_PROFILE", "work"),
    ] {
        unsafe { std::env::set_var(name, value) };
        assert!(!bundled.requires_registration("release"));
        unsafe { std::env::remove_var(name) };
    }
    for (name, value) in variables.into_iter().zip(previous) {
        unsafe {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}
