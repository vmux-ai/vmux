use std::path::PathBuf;
use vmux_service::bundle::{EMBEDDED_AGENT_LABEL, EMBEDDED_AGENT_PLIST};
use vmux_service::registry::{Backend, RegistrationStep, choose_backend};

#[test]
fn bundled_path_chooses_sm_app_service() {
    let exe = PathBuf::from(
        "/Applications/Vmux.app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service",
    );
    assert!(matches!(choose_backend(&exe), Backend::SmAppService { .. }));
}

#[test]
fn unbundled_path_chooses_launchctl() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_service");
    assert!(matches!(choose_backend(&exe), Backend::Launchctl));
}

#[test]
fn sm_app_service_registration_plan_is_complete_and_ordered() {
    let exe = PathBuf::from(
        "/Applications/Vmux.app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service",
    );

    assert_eq!(
        choose_backend(&exe).registration_steps(),
        [
            RegistrationStep::CleanupLegacy,
            RegistrationStep::UnregisterMainApp,
            RegistrationStep::UnregisterEmbeddedAgent,
            RegistrationStep::RegisterEmbeddedAgent,
            RegistrationStep::KickstartEmbeddedAgent,
        ]
    );
}

#[test]
fn launchctl_registration_plan_uses_profile_agent() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_service");

    assert_eq!(
        choose_backend(&exe).registration_steps(),
        [RegistrationStep::EnsureLaunchAgent]
    );
}

#[test]
fn embedded_agent_label_matches_packaging_plist() {
    let plist = include_str!("../../../../packaging/macos/ai.vmux.service.plist");
    let needle = format!("<string>{EMBEDDED_AGENT_LABEL}</string>");
    assert!(
        plist.contains(&needle),
        "EMBEDDED_AGENT_LABEL ({EMBEDDED_AGENT_LABEL}) must match the <Label> in {EMBEDDED_AGENT_PLIST}"
    );
}
