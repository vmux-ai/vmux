#[test]
fn info_plist_marks_app_as_ui_element() {
    let xml = include_str!("../../../packaging/macos/Info.plist");
    assert!(xml.contains("<key>LSUIElement</key>"));
    let after_key = xml.split("<key>LSUIElement</key>").nth(1).unwrap();
    assert!(
        after_key.trim_start().starts_with("<true/>"),
        "LSUIElement must be true so Vmux runs as menu-bar-only"
    );
}
