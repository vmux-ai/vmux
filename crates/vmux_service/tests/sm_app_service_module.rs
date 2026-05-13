#[cfg(target_os = "macos")]
#[test]
fn sm_app_service_module_exposes_register_main_app() {
    let _: fn() -> Result<(), vmux_service::sm_app_service::SmError> =
        vmux_service::sm_app_service::register_main_app;
}
