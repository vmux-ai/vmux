use super::*;

fn node(tag: &str, text: &str, attrs: &[(&str, &str)], bounds: [i32; 4]) -> RawDomNode {
    RawDomNode {
        tag: tag.to_string(),
        text: text.to_string(),
        value: String::new(),
        attrs: attrs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        bounds,
    }
}

fn raw_vp(nodes: Vec<RawDomNode>, viewport: Option<RawViewport>) -> RawSnapshot {
    RawSnapshot {
        url: "https://example.com".to_string(),
        title: "Example".to_string(),
        nodes,
        viewport,
    }
}

fn raw(nodes: Vec<RawDomNode>) -> RawSnapshot {
    raw_vp(nodes, None)
}

#[test]
fn viewport_passes_through_and_marks_in_viewport_by_bbox() {
    let vp = RawViewport {
        scroll_x: 0,
        scroll_y: 0,
        width: 800,
        height: 600,
        page_width: 800,
        page_height: 4000,
    };
    let on = node("button", "On", &[], [0, 10, 100, 30]);
    let off = node("button", "Off", &[], [0, 2000, 100, 30]);
    let snap = shape_snapshot(raw_vp(vec![on, off], Some(vp)));
    let on_n = snap.nodes.iter().find(|n| n.name == "On").unwrap();
    let off_n = snap.nodes.iter().find(|n| n.name == "Off").unwrap();
    assert!(on_n.in_viewport);
    assert!(!off_n.in_viewport);
    let v = snap.viewport.unwrap();
    assert_eq!(v.height, 600);
    assert_eq!(v.page_height, 4000);
}

#[test]
fn no_viewport_means_nodes_default_in_viewport_false_and_field_absent() {
    let snap = shape_snapshot(raw_vp(vec![node("button", "X", &[], [0, 0, 10, 10])], None));
    assert!(snap.viewport.is_none());
    assert!(!snap.nodes[0].in_viewport);
}

#[test]
fn skips_plain_container_without_role_or_text() {
    let snap = shape_snapshot(raw(vec![node("div", "", &[], [0, 0, 100, 40])]));
    assert!(snap.nodes.is_empty());
}

#[test]
fn keeps_button_with_role_and_name_from_text() {
    let snap = shape_snapshot(raw(vec![node("button", "Sign in", &[], [1, 2, 80, 30])]));
    assert_eq!(snap.nodes.len(), 1);
    let n = &snap.nodes[0];
    assert_eq!(n.reference, 0);
    assert_eq!(n.role, "button");
    assert_eq!(n.name, "Sign in");
    assert_eq!(n.bbox, [1, 2, 80, 30]);
}

#[test]
fn input_email_maps_to_textbox_with_placeholder_name() {
    let mut email = node(
        "input",
        "",
        &[("type", "email"), ("placeholder", "Email")],
        [0, 0, 200, 30],
    );
    email.value = "a@b.com".to_string();
    let snap = shape_snapshot(raw(vec![email]));
    let n = &snap.nodes[0];
    assert_eq!(n.role, "textbox");
    assert_eq!(n.name, "Email");
    assert_eq!(n.value.as_deref(), Some("a@b.com"));
}

#[test]
fn password_input_value_is_redacted() {
    let mut pw = node("input", "", &[("type", "password")], [0, 0, 200, 30]);
    pw.value = "hunter2".to_string();
    let snap = shape_snapshot(raw(vec![pw]));
    assert_eq!(snap.nodes[0].role, "textbox");
    assert_eq!(snap.nodes[0].value, None);
}

#[test]
fn aria_label_beats_inner_text() {
    let snap = shape_snapshot(raw(vec![node(
        "a",
        "click here",
        &[("aria-label", "Home")],
        [0, 0, 50, 20],
    )]));
    assert_eq!(snap.nodes[0].role, "link");
    assert_eq!(snap.nodes[0].name, "Home");
}

#[test]
fn disabled_and_required_become_state_flags() {
    let snap = shape_snapshot(raw(vec![node(
        "button",
        "Go",
        &[("disabled", ""), ("required", "")],
        [0, 0, 40, 20],
    )]));
    assert!(snap.nodes[0].state.contains(&"disabled".to_string()));
    assert!(snap.nodes[0].state.contains(&"required".to_string()));
}

#[test]
fn zero_area_node_is_hidden_and_skipped() {
    let snap = shape_snapshot(raw(vec![node("button", "Hidden", &[], [0, 0, 0, 0])]));
    assert!(snap.nodes.is_empty());
}

#[test]
fn refs_are_sequential_and_truncation_sets_flag() {
    let mut nodes = Vec::new();
    for i in 0..(SNAPSHOT_NODE_CAP + 5) {
        nodes.push(node("button", &format!("b{i}"), &[], [0, 0, 10, 10]));
    }
    let snap = shape_snapshot(raw(nodes));
    assert_eq!(snap.nodes.len(), SNAPSHOT_NODE_CAP);
    assert!(snap.truncated);
    assert_eq!(snap.nodes[0].reference, 0);
    assert_eq!(snap.nodes[1].reference, 1);
}

#[test]
fn role_attribute_overrides_tag() {
    let snap = shape_snapshot(raw(vec![node(
        "div",
        "Menu",
        &[("role", "button")],
        [0, 0, 30, 30],
    )]));
    assert_eq!(snap.nodes[0].role, "button");
}

#[test]
fn raw_snapshot_round_trips_through_json() {
    let original = raw(vec![node(
        "button",
        "Go",
        &[("role", "button")],
        [1, 2, 3, 4],
    )]);
    let json = serde_json::to_string(&original).unwrap();
    let parsed: RawSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, original);
}
