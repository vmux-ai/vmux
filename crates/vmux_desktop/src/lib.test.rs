use super::*;

#[test]
fn primary_window_enables_ime_input() {
    let window = primary_window_config("Vmux".to_string());

    assert!(window.ime_enabled);
}

#[test]
fn primary_window_starts_hidden_when_native_glass_needs_backdrop_setup() {
    let window = primary_window_config("Vmux".to_string());

    assert_eq!(
        window.visible,
        !cfg!(all(target_os = "macos", feature = "native-glass"))
    );
}

#[test]
fn primary_window_defaults_to_centered_default_size() {
    let window = primary_window_config("Vmux".to_string());

    assert!(matches!(
        window.position,
        WindowPosition::Centered(MonitorSelection::Primary)
    ));
    assert_eq!(window.resolution.physical_width(), DEFAULT_WINDOW_WIDTH);
    assert_eq!(window.resolution.physical_height(), DEFAULT_WINDOW_HEIGHT);
}

#[test]
fn window_plugin_keeps_app_alive_after_last_window_closes() {
    let source = include_str!("lib.rs");
    assert!(
        source.contains("ExitCondition::DontExit"),
        "WindowPlugin must opt out of automatic exit so Vmux.app survives last-window-close"
    );
}

#[test]
fn desktop_uses_single_layout_crate_for_cef_and_layout() {
    let source = include_str!("lib.rs");

    assert!(source.contains("vmux_layout::"));
    assert!(!source.contains(&["vmux_layout", "::footer"].concat()));
    assert!(!source.contains(&["vmux_", "header::HeaderPlugin"].concat()));
    assert!(!source.contains(&["vmux_", "side_sheet::SideSheetPlugin"].concat()));
}

#[test]
fn dev_build_has_no_tick_logger() {
    let source = include_str!("lib.rs");

    assert!(!source.contains(&["app", ".update", "():"].concat()));
}
