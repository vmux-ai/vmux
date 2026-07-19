#[test]
fn tab_close_button_captures_mouse_down_before_click() {
    let tab = tab_component_source();
    let close_button = tab
        .split("aria_label: \"Close tab\"")
        .nth(1)
        .and_then(|rest| rest.split("Icon { class: \"h-2.5 w-2.5\"").next())
        .expect("tab close button");

    assert!(close_button.contains("onmousedown"));
    assert!(close_button.contains("evt.prevent_default()"));
    assert!(close_button.contains("evt.stop_propagation()"));
}

#[test]
fn tab_close_button_requests_tab_close() {
    let tab = tab_component_source();
    let close_button = tab
        .split("aria_label: \"Close tab\"")
        .nth(1)
        .and_then(|rest| rest.split("Icon { class: \"h-2.5 w-2.5\"").next())
        .expect("tab close button");

    assert!(close_button.contains("command: \"close\".to_string()"));
    assert!(!close_button.contains("command: \"close_stack\".to_string()"));
}

#[test]
fn tab_hover_area_switches_tab() {
    let tab = tab_component_source();
    let tab_root = tab
        .split("div {\n            class: \"{tab_class}\"")
        .nth(1)
        .and_then(|rest| rest.split("aria_label: \"Close tab\"").next())
        .expect("tab root");

    assert!(tab_root.contains("onclick: move |_|"));
    assert!(tab_root.contains("command: \"switch\".to_string()"));
}

#[test]
fn header_tabs_use_same_fixed_width_for_active_and_inactive_states() {
    let tab = tab_component_source();

    let footprint = "group flex h-10 w-52 min-w-52 max-w-52 basis-52 shrink-0 grow-0 -mb-[3px] pb-[3px] cursor-pointer items-center gap-2 px-3.5";
    assert!(tab.contains(footprint));
    assert!(!tab.contains("max-w-[200px]"));
    assert!(!tab.contains("w-[200px]"));
    assert_eq!(tab.matches(footprint).count(), 1);
    assert_eq!(tab.matches("{tab_box_classes}").count(), 2);
    assert!(!tab.contains("width:200px"));
    assert!(!tab.contains("flex:0 0"));
    assert!(tab.contains("before:-left-2"));
    assert!(tab.contains("after:-right-2"));
    assert!(tab.contains("class: \"flex min-w-0 flex-1 items-center gap-2.5 overflow-hidden\""));
    assert!(tab.contains("min-w-0 flex-1 {trunc} text-ui"));
    assert!(tab.contains("dir_truncate_class(&display_title)"));
}

#[test]
fn inactive_tabs_add_horizontal_padding_on_hover() {
    let tab = tab_component_source();
    let inactive_branch = tab
        .split("} else {\n        (\n            String::new(),")
        .nth(1)
        .and_then(|rest| rest.split(")\n    };").next())
        .expect("inactive tab branch");
    let active_branch = tab
        .split("if is_active {")
        .nth(1)
        .and_then(|rest| rest.split("} else {").next())
        .expect("active tab branch");

    assert!(inactive_branch.contains("hover:px-4"));
    assert!(!active_branch.contains("hover:px-4"));
}

#[test]
fn side_sheet_close_button_tracks_stack_row_hover() {
    let row = side_sheet_stack_row_component_source();

    assert!(row.contains("let mut hovered = use_signal(|| false)"));
    assert!(row.contains("onmouseenter: move |_| hovered.set(true)"));
    assert!(row.contains("onmouseleave: move |_| hovered.set(false)"));
    assert!(row.contains("class: if hovered()"));
    assert!(row.contains("opacity-100"));
    assert!(row.contains("opacity-0"));
    assert!(!row.contains("group-hover:opacity-100"));
    assert!(!row.contains("group-hover/stack:opacity-100"));
}

#[test]
fn knowledge_side_sheet_opens_markdown_tree_through_file_pages() {
    let source = include_str!("../src/page.rs");

    assert!(source.contains("KNOWLEDGE_TREE_EVENT"));
    assert!(source.contains("KnowledgeEntryRow"));
    assert!(source.contains("open_knowledge_path"));
    assert!(!source.contains("KnowledgeUse"));
    assert!(!source.contains("Build with"));
    assert!(!source.contains("vmux://notes"));
}

#[test]
fn folded_pane_shows_its_active_stack() {
    let pane = pane_section_component_source();

    assert!(pane.contains(".find(|stack| stack.is_active"));
    assert!(pane.contains("if folded()"));
    assert!(pane.contains("if let Some(stack) = folded_stack"));
    assert!(pane.contains("SideSheetStackRow { stack, pane_id }"));
}

#[test]
fn embedded_header_css_has_fixed_tab_utilities() {
    let css_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../vmux_server/dist/assets/index.css");
    let Ok(css) = std::fs::read_to_string(css_path) else {
        return;
    };

    for selector in [
        ".w-52{width:",
        ".min-w-52{min-width:",
        ".max-w-52{max-width:",
        ".basis-52{flex-basis:",
        ".grow-0{flex-grow:0}",
        ".before\\:-left-2:before",
        ".after\\:-right-2:after",
    ] {
        assert!(css.contains(selector), "missing {selector}");
    }
}

#[test]
fn layout_page_offsets_header_and_side_sheet_by_window_padding() {
    let source = include_str!("../src/page.rs");

    assert!(source.contains("--vmux-header-top"));
    assert!(source.contains("--vmux-side-sheet-left"));
    assert!(source.contains("--vmux-side-sheet-top"));
    assert!(source.contains("--vmux-side-sheet-bottom"));
    assert!(source.contains("top-[var(--vmux-header-top)]"));
    assert!(source.contains("left-[var(--vmux-side-sheet-left)]"));
}

#[test]
fn layout_page_uses_computed_header_right_offset() {
    let source = include_str!("../src/page.rs");

    assert!(source.contains("state.header_right()"));
    assert!(source.contains("--vmux-header-right"));
}

#[test]
fn persistent_layout_state_has_no_infinite_animation() {
    let source = include_str!("../src/page.rs");

    for class in ["animate-pulse", "animate-ping", "animate-bounce"] {
        assert!(!source.contains(class), "persistent layout uses {class}");
    }
}

#[test]
fn layout_document_disables_both_scroll_axes() {
    let css = include_str!("../assets/index.css");

    assert!(css.contains("overflow-hidden"));
}

#[test]
fn layout_page_gates_header_and_side_sheet_until_host_state_arrives() {
    let source = include_str!("../src/page.rs");

    assert!(source.contains("layout_overlay_ready"));
    assert!(source.contains("let overlay_ready = layout_overlay_ready"));
    assert!(source.contains("if overlay_ready && state.side_sheet_open"));
    assert!(source.contains("if overlay_ready && state.header_visible()"));
}

#[test]
fn header_url_row_uses_glass_instead_of_page_bg_color() {
    let source = include_str!("../src/page.rs");
    let url_row = source
        .split("fn url_row_cef")
        .nth(1)
        .and_then(|rest| rest.split("#[component]\nfn HeaderAddressBar").next())
        .expect("url row helper");

    assert!(url_row.contains("bg-glass"));
    assert!(!url_row.contains("bg-[var(--vmux-url-bg)]"));
    assert!(!url_row.contains("--vmux-url-bg:{color};"));
}

#[test]
fn active_tab_uses_glass_instead_of_page_bg_color() {
    let tab = tab_component_source();

    assert!(tab.contains("glass rounded-t-md"));
    assert!(!tab.contains("bg-[var(--tab-bg)]"));
    assert!(!tab.contains("--tab-bg:{color};"));
}

#[test]
fn command_bar_page_installs_document_pointer_dismiss_listener() {
    let source = include_str!("../src/command_bar/page.rs");

    assert!(source.contains("install_command_bar_outside_pointer_listener"));
    assert!(source.contains("\"pointerdown\""));
    assert!(source.contains("command-bar-shell"));
    assert!(source.contains("shell.contains"));
    assert!(source.contains("emit_action(\"dismiss\", \"\")"));
}

#[test]
fn dir_path_titles_truncate_at_start() {
    let source = include_str!("../src/page.rs");

    assert!(source.contains("fn dir_truncate_class(title: &str) -> &'static str"));
    assert!(source.contains("title.starts_with('/') || title.starts_with(\"~/\")"));
    assert!(source.contains("\"truncate-start\""));
    assert!(source.contains("dir_truncate_class(&display_title)"));
    assert!(source.contains("dir_truncate_class(&stack.title)"));

    let theme = include_str!("../../vmux_ui/assets/theme.css");
    assert!(theme.contains(".truncate-start"));
    assert!(theme.contains("direction: rtl"));
    assert!(theme.contains("text-overflow: ellipsis"));

    let server_css = include_str!("../../vmux_server/assets/index.css");
    assert!(server_css.contains("../../vmux_ui/assets/theme.css"));
    assert!(server_css.contains("@source \"../../vmux_layout/src\""));
}

#[test]
fn bookmark_drag_uses_pointer_capture_instead_of_html_drag() {
    let source = include_str!("../src/page.rs");

    assert!(source.contains("fn begin_bookmark_drag"));
    assert!(source.contains("fn update_bookmark_drag"));
    assert!(source.contains("fn end_bookmark_drag"));
    assert!(source.contains("set_pointer_capture"));
    assert!(source.contains("onpointermove"));
    assert!(source.contains("onpointerup"));
    assert!(!source.contains("DragData"));
    assert!(!source.contains("draggable: \"true\""));
}

fn tab_component_source() -> &'static str {
    include_str!("../src/page.rs")
        .split("fn Tab(tab: TabRow)")
        .nth(1)
        .and_then(|rest| rest.split("fn NewTabButton()").next())
        .expect("tab component")
}

fn side_sheet_stack_row_component_source() -> &'static str {
    include_str!("../src/page.rs")
        .split("fn SideSheetStackRow(stack: StackNode, pane_id: u64)")
        .nth(1)
        .and_then(|rest| rest.split("fn download_pct").next())
        .expect("side sheet stack row component")
}

fn pane_section_component_source() -> &'static str {
    include_str!("../src/page.rs")
        .split("fn PaneSection(pane: PaneNode, index: usize)")
        .nth(1)
        .and_then(|rest| rest.split("fn NewStackRow").next())
        .expect("pane section component")
}
