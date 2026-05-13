use std::path::PathBuf;
use vmux_service::service_registration::{Backend, choose_backend};

#[test]
fn bundled_path_chooses_sm_app_service() {
    let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/vmux_service");
    assert!(matches!(choose_backend(&exe), Backend::SmAppService { .. }));
}

#[test]
fn unbundled_path_chooses_launchctl() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_service");
    assert!(matches!(choose_backend(&exe), Backend::Launchctl));
}
