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

pub const SNAPSHOT_NODE_CAP: usize = 600;
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
pub struct RawViewport {
    #[serde(rename = "scrollX")]
    pub scroll_x: i32,
    #[serde(rename = "scrollY")]
    pub scroll_y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(rename = "pageWidth")]
    pub page_width: i32,
    #[serde(rename = "pageHeight")]
    pub page_height: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawSnapshot {
    pub url: String,
    pub title: String,
    pub nodes: Vec<RawDomNode>,
    #[serde(default)]
    pub viewport: Option<RawViewport>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Viewport {
    #[serde(rename = "scrollX")]
    pub scroll_x: i32,
    #[serde(rename = "scrollY")]
    pub scroll_y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(rename = "pageWidth")]
    pub page_width: i32,
    #[serde(rename = "pageHeight")]
    pub page_height: i32,
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
    #[serde(rename = "inViewport", skip_serializing_if = "is_false")]
    pub in_viewport: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub state: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
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
            in_viewport: raw
                .viewport
                .as_ref()
                .map(|vp| in_viewport_of(raw_node.bounds, vp))
                .unwrap_or(false),
            state: derive_state(raw_node),
        });
    }
    Snapshot {
        url: raw.url,
        title: raw.title,
        viewport: raw.viewport.map(|v| Viewport {
            scroll_x: v.scroll_x,
            scroll_y: v.scroll_y,
            width: v.width,
            height: v.height,
            page_width: v.page_width,
            page_height: v.page_height,
        }),
        nodes,
        truncated,
    }
}

fn in_viewport_of(bbox: [i32; 4], vp: &RawViewport) -> bool {
    let (x, y, w, h) = (bbox[0], bbox[1], bbox[2], bbox[3]);
    let intersects_x = x < vp.width && (x + w) > 0;
    let intersects_y = y < vp.height && (y + h) > 0;
    intersects_x && intersects_y
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
#[path = "dom_snapshot.test.rs"]
mod tests;
