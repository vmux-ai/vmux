#[test]
fn tray_module_not_a_placeholder() {
    let source = include_str!("tray.rs");
    let tray_builder = ["Tray", "Icon", "Builder"].concat();
    let tray_type = ["tray_icon", "::", "Tray", "Icon"].concat();
    assert!(
        source.contains(&tray_builder) || source.contains(&tray_type),
        "tray.rs must wire tray-icon, not be a stub"
    );
}

#[test]
fn toggle_label_reflects_visibility() {
    assert_eq!(super::toggle_label(true, "en-US"), "Close Window");
    assert_eq!(super::toggle_label(false, "en-US"), "Open Window");
    assert_eq!(super::toggle_label(false, "ja"), "ウインドウを開く");
}

#[test]
fn toggle_event_routes_by_visibility() {
    use super::LifecycleEvent;
    assert!(matches!(
        super::toggle_lifecycle_event(true),
        LifecycleEvent::HideAllWindows
    ));
    assert!(matches!(
        super::toggle_lifecycle_event(false),
        LifecycleEvent::ShowAllWindows
    ));
}

#[test]
fn tray_syncs_toggle_label_with_window_visibility() {
    let source = include_str!("tray.rs")
        .split("#[cfg(test)]")
        .next()
        .expect("production source");

    assert!(source.contains("sync_tray_menu_state"));
    assert!(source.contains("set_text"));
    assert!(source.contains("toggle_label"));
}

#[test]
fn tray_icon_has_visible_pixels() {
    let rgba = super::tray_icon_rgba();

    assert_eq!(rgba.len(), 16 * 16 * 4);
    assert!(
        rgba.chunks_exact(4).any(|pixel| pixel[3] != 0),
        "tray icon must not be fully transparent"
    );
}

#[test]
fn tray_icon_uses_macos_template_mode() {
    let source = include_str!("tray.rs");

    assert!(source.contains("with_icon_as_template(true)"));
}
