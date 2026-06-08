use std::path::PathBuf;
use vmux_service::registry::{StartMode, start_mode_for};

#[test]
fn bundled_service_app_registers() {
    let exe = PathBuf::from(
        "/Applications/Vmux.app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service",
    );
    assert_eq!(start_mode_for(&exe), StartMode::Register);
}

#[test]
fn bundled_main_app_registers() {
    let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");
    assert_eq!(start_mode_for(&exe), StartMode::Register);
}

#[test]
fn dev_target_debug_spawns_detached() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_service");
    assert_eq!(start_mode_for(&exe), StartMode::SpawnDetached);
}

#[test]
fn plain_bin_spawns_detached() {
    let exe = PathBuf::from("/usr/local/bin/vmux_service");
    assert_eq!(start_mode_for(&exe), StartMode::SpawnDetached);
}
