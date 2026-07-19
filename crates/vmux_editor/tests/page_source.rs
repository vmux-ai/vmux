fn explorer_source() -> &'static str {
    include_str!("../src/explorer.rs")
}

fn page_source() -> &'static str {
    include_str!("../src/page.rs")
}

fn note_source() -> &'static str {
    include_str!("../src/note.rs")
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
    assert!(s.contains("merge_tree_motion_rows"));
    assert!(s.contains("grid-rows-[0fr]"));
    assert!(s.contains("grid-rows-[1fr]"));
    assert!(s.contains("transition-[opacity,translate]"));
    assert!(!s.contains("transition-[grid-template-rows,opacity,translate]"));
    assert!(s.contains("transition-[rotate] duration-150"));
    assert!(s.contains("schedule_tree_focus"));
    assert!(s.contains("current_path"));
}

#[test]
fn page_mounts_panel_and_wires_toggle() {
    let s = page_source();
    assert!(s.contains("ExplorerPanel { visible }"));
    assert!(s.contains("ExplorerPanelSetVisible"));
    assert!(s.contains("ExplorerChromeEvent"));
    assert!(explorer_source().contains("ExplorerFocusEvent"));
    assert!(s.contains("ExplorerPanelWidth"));
    assert!(s.contains("ExplorerSidebar"));
    assert!(s.contains("-translate-x-full"));
    assert!(s.contains("transition-[translate,opacity]"));
    assert!(!s.contains("transition-[width] duration-200"));
    assert!(s.contains("width:0px;contain:layout style;"));
    assert!(s.contains("relative z-[2] h-full shrink-0"));
    assert!(s.contains("ExplorerRevealCurrent"));
    assert!(s.contains("handle_explorer_shortcut"));
    assert!(s.contains("Mode::Text => focus_file_input()"));
    assert!(s.contains("raw.shift_key()"));
    assert!(s.contains("key.eq_ignore_ascii_case(\"e\")"));
}

#[test]
fn page_wires_shared_note_editor_diff_toggle() {
    let s = page_source();
    assert!(s.contains("FileViewModeEvent"));
    assert!(s.contains("FileViewModeSet"));
    assert!(s.contains("FileViewMode::Note"));
    assert!(s.contains("let mut file_view_mode = use_signal(|| FileViewMode::Note)"));
    assert!(s.contains("Rendered Markdown with live editing"));
    assert!(s.contains("file_view_mode.set(FileViewMode::Editor)"));
    assert!(s.contains("file_view_mode.set(FileViewMode::Diff)"));
    assert!(s.contains("file-mode-note-enter"));
    assert!(s.contains("file-mode-editor-enter"));
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
    assert!(s.contains("render_block(&note_block.block, index)"));
    assert!(s.contains("document.set_title(&title)"));
    assert!(s.contains("let mut note_editing = use_signal(|| false)"));
    assert!(s.contains("if note_editing() && Some(index as u32) == active"));
    assert!(s.contains("note_pointer_line(&event, start, end)"));
    assert!(s.contains("note_pointer_col(&event, &pointer_raw)"));
    assert!(s.contains("query_selector(\"[data-note-line-text]\")"));
    assert!(s.contains("class: \"flow-root w-full cursor-text\""));
    assert!(
        !s.contains("note_editing.set(false);\n                            focus_container();")
    );
    assert!(s.contains("class: \"relative w-full cursor-text\""));
    assert!(!s.contains("rounded-lg bg-primary/[0.04]"));
    assert!(s.contains("if git_path() != m.abs_path"));
}

#[test]
fn note_lists_render_every_block_with_stable_keys() {
    let s = note_source();
    assert!(s.contains("render_block(block, block_index)"));
    assert!(!s.contains("if let MdBlock::Paragraph { inlines } = block"));
}
