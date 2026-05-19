use std::path::PathBuf;
use vmux_service::bundle::{EMBEDDED_AGENT_LABEL, EMBEDDED_AGENT_PLIST};
use vmux_service::registry::{Backend, choose_backend};

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

#[test]
fn ensure_running_calls_legacy_cleanup_for_sm_app_service_path() {
    let source = include_str!("../src/registry.rs");
    assert!(
        source.contains("cleanup_legacy_registrations"),
        "SmAppService branch must invoke legacy cleanup"
    );
}

#[test]
fn ensure_running_kickstarts_after_register_for_sm_app_service_path() {
    let source = include_str!("../src/registry.rs");
    assert!(
        source.contains("crate::launchd::kickstart(bundle::EMBEDDED_AGENT_LABEL)"),
        "SmAppService branch must kickstart the embedded agent so it actually runs after registration"
    );
}

#[test]
fn embedded_agent_label_matches_packaging_plist() {
    let plist = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    let needle = format!("<string>{EMBEDDED_AGENT_LABEL}</string>");
    assert!(
        plist.contains(&needle),
        "EMBEDDED_AGENT_LABEL ({EMBEDDED_AGENT_LABEL}) must match the <Label> in {EMBEDDED_AGENT_PLIST}"
    );
}
