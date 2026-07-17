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
    assert!(s.contains("ExplorerTreePrefetch"));
    assert!(s.contains("ExplorerTreeRefresh"));
    assert!(s.contains("ExplorerCreate"));
    assert!(s.contains("ExplorerRename"));
    assert!(s.contains("ExplorerDelete"));
    assert!(s.contains("EXPLORER_FS_RESULT_EVENT"));
}

#[test]
fn explorer_animates_tree_and_sections() {
    let s = explorer_source();
    assert!(s.contains("reconcile_rows"));
    assert!(s.contains("grid-rows-[0fr]"));
    assert!(s.contains("grid-rows-[1fr]"));
    assert!(s.contains("transition-[grid-template-rows,opacity,translate]"));
    assert!(s.contains("transition-[rotate] duration-150"));
    assert!(s.contains("schedule_tree_focus"));
    assert!(s.contains("current_path"));
}

#[test]
fn page_mounts_panel_and_wires_toggle() {
    let s = page_source();
    assert!(s.contains("ExplorerPanel {}"));
    assert!(s.contains("ExplorerPanelSetVisible"));
    assert!(s.contains("ExplorerChromeEvent"));
    assert!(s.contains("ExplorerPanelWidth"));
    assert!(s.contains("ExplorerSidebar"));
    assert!(s.contains("-translate-x-full"));
    assert!(s.contains("transition-[translate,opacity]"));
    assert!(!s.contains("transition-[width] duration-200"));
    assert!(s.contains("ExplorerRevealCurrent"));
    assert!(s.contains("raw.shift_key()"));
    assert!(s.contains("key.eq_ignore_ascii_case(\"e\")"));
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
    assert!(s.contains("diff_marker_sign(marker)"));
    assert!(s.contains("schedule_git_refresh(git_refresh_generation, git_nonce)"));
    assert!(s.contains("GIT_CHANGED_EVENT"));
    assert!(s.contains("GitChangedEvent"));
    assert!(s.contains("reveal_git_change(git_line_markers, cell_dims)"));
    assert!(s.contains("diff_marker_text_class(marker)"));
    assert!(s.contains("diff_marker_row_class(marker)"));
    assert!(s.contains("text-ansi-3"));
    assert!(s.contains("width:calc(var(--cw, 1ch) * {gw});"));
    let line_number = s.find("\"{ln + 1}\"").unwrap();
    let marker_sign = s.find("\"{diff_marker_sign(marker)}\"").unwrap();
    assert!(line_number < marker_sign);
    assert!(s.contains("transition-transform duration-150"));
    assert!(s.contains("if git_path() != m.abs_path"));
}
