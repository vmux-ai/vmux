use super::*;

#[test]
fn hidden_within_timeout_does_nothing() {
    assert_eq!(
        splash_decision(false, false, Duration::from_secs(1)),
        SplashAction::None
    );
}

#[test]
fn visible_triggers_fade() {
    assert_eq!(
        splash_decision(true, false, Duration::from_secs(1)),
        SplashAction::Fade
    );
}

#[test]
fn hidden_past_timeout_forces_dismiss() {
    assert_eq!(
        splash_decision(false, false, Duration::from_secs(20)),
        SplashAction::Force
    );
}

#[test]
fn dismissed_is_idempotent() {
    assert_eq!(
        splash_decision(true, true, Duration::from_secs(1)),
        SplashAction::None
    );
    assert_eq!(
        splash_decision(false, true, Duration::from_secs(99)),
        SplashAction::None
    );
}

#[test]
fn splash_plugin_registered_by_native_window_plugin() {
    let mut app = App::new();
    app.add_plugins(crate::plugins::NativeWindowPlugin);

    assert!(app.is_plugin_added::<SplashPlugin>());
}

#[test]
fn splash_uses_spinner_and_version_detected_material() {
    let source = include_str!("splash.rs");
    assert!(source.contains("NSProgressIndicator"));
    assert!(source.contains("AnyClass::get(c\"NSGlassEffectView\")"));
    assert!(source.contains("NSVisualEffectView"));
}

#[test]
fn splash_shows_title_and_status_label() {
    let source = include_str!("splash.rs");
    assert!(source.contains("NSTextField"));
    assert!(source.contains("\"Vmux\""));
    assert!(source.contains("SplashStatus"));
    assert!(source.contains("update_splash_text"));
}

#[test]
fn splash_panel_is_fullscreen_auxiliary() {
    let source = include_str!("splash.rs");
    assert!(source.contains("setCollectionBehavior"));
    assert!(source.contains("FullScreenAuxiliary"));
    assert!(source.contains("CanJoinAllSpaces"));
}

#[test]
fn desktop_enables_splash_appkit_features() {
    let manifest = include_str!("../Cargo.toml");
    assert!(manifest.contains("\"objc2-app-kit/NSProgressIndicator\""));
    assert!(manifest.contains("\"objc2-app-kit/NSVisualEffectView\""));
    assert!(manifest.contains("\"objc2-app-kit/NSTextField\""));
    assert!(manifest.contains("\"objc2-app-kit/NSFont\""));
}
