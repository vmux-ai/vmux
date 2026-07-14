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

#[test]
fn page_wires_shared_editor_diff_toggle() {
    let s = page_source();
    assert!(s.contains("FileViewModeEvent"));
    assert!(s.contains("FileViewModeSet"));
    assert!(s.contains("Show diffs in all open files"));
    assert!(s.contains("file_view_mode.set(next)"));
    assert!(s.contains("if next == FileViewMode::Diff"));
    assert!(s.contains("visible: file_view_mode() == FileViewMode::Diff"));
    assert!(s.contains("markers: git_line_markers"));
    assert!(s.contains("diff_marker_class(marker)"));
    assert!(s.contains("schedule_git_refresh(git_refresh_generation, git_nonce)"));
    assert!(s.contains("transition-transform duration-150"));
    assert!(s.contains("if git_path() != m.abs_path"));
}
