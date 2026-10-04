use serde::{Deserialize, Serialize};

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

const INTERACTIVE_TAGS: &[&str] = &[
    "a", "button", "input", "select", "textarea", "option", "summary", "label",
];
const LANDMARK_TAGS: &[&str] = &[
    "nav", "main", "header", "footer", "aside", "h1", "h2", "h3", "h4", "h5", "h6",
];

impl From<RawSnapshot> for Snapshot {
    fn from(raw: RawSnapshot) -> Self {
        let mut nodes = Vec::new();
        let mut truncated = false;
        for raw_node in &raw.nodes {
            if !raw_node.is_interesting() {
                continue;
            }
            if nodes.len() >= SNAPSHOT_NODE_CAP {
                truncated = true;
                break;
            }
            let reference = nodes.len() as u32;
            nodes.push(SnapNode {
                reference,
                role: raw_node.role(),
                name: raw_node.name(),
                value: raw_node.value(),
                bbox: raw_node.bounds,
                in_viewport: raw
                    .viewport
                    .as_ref()
                    .is_some_and(|viewport| viewport.contains(raw_node.bounds)),
                state: raw_node.state(),
            });
        }
        Self {
            url: raw.url,
            title: raw.title,
            viewport: raw.viewport.map(|viewport| Viewport {
                scroll_x: viewport.scroll_x,
                scroll_y: viewport.scroll_y,
                width: viewport.width,
                height: viewport.height,
                page_width: viewport.page_width,
                page_height: viewport.page_height,
            }),
            nodes,
            truncated,
        }
    }
}

impl RawViewport {
    fn contains(&self, bbox: [i32; 4]) -> bool {
        let (x, y, width, height) = (bbox[0], bbox[1], bbox[2], bbox[3]);
        x < self.width && x + width > 0 && y < self.height && y + height > 0
    }
}

impl RawDomNode {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    fn has_attr(&self, key: &str) -> bool {
        self.attrs.iter().any(|(name, _)| name == key)
    }

    fn is_interesting(&self) -> bool {
        if self.bounds[2] * self.bounds[3] <= 0 {
            return false;
        }
        let tag = self.tag.as_str();
        if INTERACTIVE_TAGS.contains(&tag) {
            return true;
        }
        if self.has_attr("role") || self.has_attr("tabindex") || self.has_attr("aria-label") {
            return true;
        }
        LANDMARK_TAGS.contains(&tag) && !self.text.trim().is_empty()
    }

    fn role(&self) -> String {
        if let Some(role) = self.attr("role")
            && !role.is_empty()
        {
            return role.to_string();
        }
        match self.tag.as_str() {
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
            "input" => match self.attr("type").unwrap_or("text") {
                "checkbox" => "checkbox".to_string(),
                "radio" => "radio".to_string(),
                "submit" | "button" | "reset" => "button".to_string(),
                "range" => "slider".to_string(),
                _ => "textbox".to_string(),
            },
            other => other.to_string(),
        }
    }

    fn name(&self) -> String {
        let candidate = self
            .attr("aria-label")
            .filter(|value| !value.trim().is_empty())
            .or_else(|| self.attr("alt").filter(|value| !value.trim().is_empty()))
            .or_else(|| self.attr("title").filter(|value| !value.trim().is_empty()))
            .or_else(|| {
                self.attr("placeholder")
                    .filter(|value| !value.trim().is_empty())
            })
            .map(str::to_string)
            .unwrap_or_else(|| self.text.trim().to_string());
        let mut name = candidate.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.chars().count() > SNAPSHOT_NAME_CAP {
            name = name.chars().take(SNAPSHOT_NAME_CAP).collect();
        }
        name
    }

    fn value(&self) -> Option<String> {
        match self.tag.as_str() {
            "input" if self.attr("type") == Some("password") => None,
            "input" | "textarea" | "select" => Some(self.value.clone()),
            _ => None,
        }
    }

    fn state(&self) -> Vec<String> {
        let mut state = Vec::new();
        for flag in ["disabled", "required", "checked"] {
            if self.has_attr(flag) {
                state.push(flag.to_string());
            }
        }
        if self.attr("aria-expanded") == Some("true") {
            state.push("expanded".to_string());
        }
        if self.attr("aria-selected") == Some("true") {
            state.push("selected".to_string());
        }
        state
    }
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
        let snap = Snapshot::from(raw_vp(vec![on, off], Some(vp)));
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
        let snap = Snapshot::from(raw_vp(vec![node("button", "X", &[], [0, 0, 10, 10])], None));
        assert!(snap.viewport.is_none());
        assert!(!snap.nodes[0].in_viewport);
    }

    #[test]
    fn skips_plain_container_without_role_or_text() {
        let snap = Snapshot::from(raw(vec![node("div", "", &[], [0, 0, 100, 40])]));
        assert!(snap.nodes.is_empty());
    }

    #[test]
    fn keeps_button_with_role_and_name_from_text() {
        let snap = Snapshot::from(raw(vec![node("button", "Sign in", &[], [1, 2, 80, 30])]));
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
        let snap = Snapshot::from(raw(vec![email]));
        let n = &snap.nodes[0];
        assert_eq!(n.role, "textbox");
        assert_eq!(n.name, "Email");
        assert_eq!(n.value.as_deref(), Some("a@b.com"));
    }

    #[test]
    fn password_input_value_is_redacted() {
        let mut pw = node("input", "", &[("type", "password")], [0, 0, 200, 30]);
        pw.value = "hunter2".to_string();
        let snap = Snapshot::from(raw(vec![pw]));
        assert_eq!(snap.nodes[0].role, "textbox");
        assert_eq!(snap.nodes[0].value, None);
    }

    #[test]
    fn aria_label_beats_inner_text() {
        let snap = Snapshot::from(raw(vec![node(
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
        let snap = Snapshot::from(raw(vec![node(
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
        let snap = Snapshot::from(raw(vec![node("button", "Hidden", &[], [0, 0, 0, 0])]));
        assert!(snap.nodes.is_empty());
    }

    #[test]
    fn refs_are_sequential_and_truncation_sets_flag() {
        let mut nodes = Vec::new();
        for i in 0..(SNAPSHOT_NODE_CAP + 5) {
            nodes.push(node("button", &format!("b{i}"), &[], [0, 0, 10, 10]));
        }
        let snap = Snapshot::from(raw(nodes));
        assert_eq!(snap.nodes.len(), SNAPSHOT_NODE_CAP);
        assert!(snap.truncated);
        assert_eq!(snap.nodes[0].reference, 0);
        assert_eq!(snap.nodes[1].reference, 1);
    }

    #[test]
    fn role_attribute_overrides_tag() {
        let snap = Snapshot::from(raw(vec![node(
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
