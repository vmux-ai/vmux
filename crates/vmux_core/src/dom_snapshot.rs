use serde::{Deserialize, Serialize};

pub const SNAPSHOT_ATTRS: &[&str] = &[
    "role",
    "aria-label",
    "aria-expanded",
    "aria-selected",
    "alt",
    "title",
    "placeholder",
    "type",
    "name",
    "href",
    "id",
    "tabindex",
    "disabled",
    "required",
    "checked",
];

pub const SNAPSHOT_NODE_CAP: usize = 300;
pub const SNAPSHOT_NAME_CAP: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawDomNode {
    pub tag: String,
    pub text: String,
    pub value: String,
    pub attrs: Vec<(String, String)>,
    pub bounds: [i32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawSnapshot {
    pub url: String,
    pub title: String,
    pub nodes: Vec<RawDomNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapNode {
    #[serde(rename = "ref")]
    pub reference: u32,
    pub role: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub bbox: [i32; 4],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub state: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub url: String,
    pub title: String,
    pub nodes: Vec<SnapNode>,
    #[serde(skip_serializing_if = "is_false")]
    pub truncated: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn shape_snapshot(raw: RawSnapshot) -> Snapshot {
    let mut nodes = Vec::new();
    let mut truncated = false;
    for raw_node in &raw.nodes {
        if !is_interesting(raw_node) {
            continue;
        }
        if nodes.len() >= SNAPSHOT_NODE_CAP {
            truncated = true;
            break;
        }
        let reference = nodes.len() as u32;
        nodes.push(SnapNode {
            reference,
            role: derive_role(raw_node),
            name: derive_name(raw_node),
            value: derive_value(raw_node),
            bbox: raw_node.bounds,
            state: derive_state(raw_node),
        });
    }
    Snapshot {
        url: raw.url,
        title: raw.title,
        nodes,
        truncated,
    }
}

fn attr<'a>(node: &'a RawDomNode, key: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn has_attr(node: &RawDomNode, key: &str) -> bool {
    node.attrs.iter().any(|(k, _)| k == key)
}

fn area(node: &RawDomNode) -> i32 {
    node.bounds[2] * node.bounds[3]
}

const INTERACTIVE_TAGS: &[&str] = &[
    "a", "button", "input", "select", "textarea", "option", "summary", "label",
];
const LANDMARK_TAGS: &[&str] = &[
    "nav", "main", "header", "footer", "aside", "h1", "h2", "h3", "h4", "h5", "h6",
];

fn is_interesting(node: &RawDomNode) -> bool {
    if area(node) <= 0 {
        return false;
    }
    let tag = node.tag.as_str();
    if INTERACTIVE_TAGS.contains(&tag) {
        return true;
    }
    if has_attr(node, "role") || has_attr(node, "tabindex") || has_attr(node, "aria-label") {
        return true;
    }
    if LANDMARK_TAGS.contains(&tag) && !node.text.trim().is_empty() {
        return true;
    }
    false
}

fn derive_role(node: &RawDomNode) -> String {
    if let Some(role) = attr(node, "role")
        && !role.is_empty()
    {
        return role.to_string();
    }
    match node.tag.as_str() {
        "a" => "link".to_string(),
        "button" | "summary" => "button".to_string(),
        "select" => "combobox".to_string(),
        "textarea" => "textbox".to_string(),
        "option" => "option".to_string(),
        "label" => "label".to_string(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading".to_string(),
        "nav" => "navigation".to_string(),
        "main" => "main".to_string(),
        "header" => "banner".to_string(),
        "footer" => "contentinfo".to_string(),
        "aside" => "complementary".to_string(),
        "input" => match attr(node, "type").unwrap_or("text") {
            "checkbox" => "checkbox".to_string(),
            "radio" => "radio".to_string(),
            "submit" | "button" | "reset" => "button".to_string(),
            "range" => "slider".to_string(),
            _ => "textbox".to_string(),
        },
        other => other.to_string(),
    }
}

fn derive_name(node: &RawDomNode) -> String {
    let candidate = attr(node, "aria-label")
        .filter(|v| !v.trim().is_empty())
        .or_else(|| attr(node, "alt").filter(|v| !v.trim().is_empty()))
        .or_else(|| attr(node, "title").filter(|v| !v.trim().is_empty()))
        .or_else(|| attr(node, "placeholder").filter(|v| !v.trim().is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| node.text.trim().to_string());
    let mut name: String = candidate.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.chars().count() > SNAPSHOT_NAME_CAP {
        name = name.chars().take(SNAPSHOT_NAME_CAP).collect();
    }
    name
}

fn derive_value(node: &RawDomNode) -> Option<String> {
    match node.tag.as_str() {
        "input" if attr(node, "type") == Some("password") => None,
        "input" | "textarea" | "select" => Some(node.value.clone()),
        _ => None,
    }
}

fn derive_state(node: &RawDomNode) -> Vec<String> {
    let mut state = Vec::new();
    for flag in ["disabled", "required", "checked"] {
        if has_attr(node, flag) {
            state.push(flag.to_string());
        }
    }
    if attr(node, "aria-expanded") == Some("true") {
        state.push("expanded".to_string());
    }
    if attr(node, "aria-selected") == Some("true") {
        state.push("selected".to_string());
    }
    state
}

#[cfg(test)]
mod tests {
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

    fn raw(nodes: Vec<RawDomNode>) -> RawSnapshot {
        RawSnapshot {
            url: "https://example.com".to_string(),
            title: "Example".to_string(),
            nodes,
        }
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
}
