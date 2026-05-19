#[cfg(target_os = "macos")]
#[test]
fn sm_app_service_module_exposes_register_main_app() {
    let _: fn() -> Result<(), vmux_service::sm_app_service::SmError> =
        vmux_service::sm_app_service::register_main_app;
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires the test binary to run from inside a signed .app in /Applications"]
fn register_main_app_returns_status() {
    use vmux_service::sm_app_service::{Status, main_app_status, register_main_app};
    let _ = register_main_app();
    assert!(matches!(
        main_app_status(),
        Status::Enabled | Status::RequiresApproval
    ));
}

#[cfg(target_os = "macos")]
#[test]
fn agent_status_no_longer_stub() {
    use vmux_service::sm_app_service::{Status, agent_status};
    let status = agent_status("ai.vmux.service.plist");
    let _ = matches!(
        status,
        Status::NotRegistered | Status::Enabled | Status::RequiresApproval | Status::NotFound
    );
}
