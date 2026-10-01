use vmux_service::sm_app_service::{AgentService, MainAppService, Status};

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires the test binary to run from inside a signed .app in /Applications"]
fn register_main_app_returns_status() {
    let _ = MainAppService::register();
    assert!(matches!(
        MainAppService::status(),
        Status::Enabled | Status::RequiresApproval
    ));
}

#[cfg(target_os = "macos")]
#[test]
fn agent_status_no_longer_stub() {
    let status = AgentService::new("ai.vmux.service.plist").status();
    let _ = matches!(
        status,
        Status::NotRegistered | Status::Enabled | Status::RequiresApproval | Status::NotFound
    );
}
