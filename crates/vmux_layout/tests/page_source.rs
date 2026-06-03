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
    assert_eq!(tab.matches("{tab_box_classes}").count(), 3);
    assert!(!tab.contains("width:200px"));
    assert!(!tab.contains("flex:0 0"));
    assert!(tab.contains("before:-left-2"));
    assert!(tab.contains("after:-right-2"));
    assert!(tab.contains("class: \"flex min-w-0 flex-1 items-center gap-2.5 overflow-hidden\""));
    assert!(tab.contains("min-w-0 flex-1 truncate text-ui"));
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

fn tab_component_source() -> &'static str {
    include_str!("../src/page.rs")
        .split("fn Tab(tab: TabRow)")
        .nth(1)
        .and_then(|rest| rest.split("fn NewTabButton()").next())
        .expect("tab component")
}
