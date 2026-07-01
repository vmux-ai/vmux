fn explorer_source() -> &'static str {
    include_str!("../src/explorer.rs")
}

fn page_source() -> &'static str {
    include_str!("../src/page.rs")
}

#[test]
fn explorer_panel_renders_three_sections() {
    let s = explorer_source();
    assert!(s.contains("\"Explorer\""));
    assert!(s.contains("Open Editors"));
    assert!(s.contains("Outline"));
}

#[test]
fn explorer_rows_emit_intents() {
    let s = explorer_source();
    assert!(s.contains("ExplorerTreeToggle"));
    assert!(s.contains("FileOpenEvent"));
    assert!(s.contains("ExplorerCloseEditor"));
    assert!(s.contains("ExplorerGoto"));
}

#[test]
fn page_mounts_panel_and_wires_toggle() {
    let s = page_source();
    assert!(s.contains("ExplorerPanel {}"));
    assert!(s.contains("ExplorerPanelToggle"));
    assert!(s.contains("ExplorerChromeEvent"));
    assert!(s.contains("ExplorerPanelWidth"));
}
