use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Space,
    Pane,
    Split,
    Tab,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    Row,
    Column,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Focus {
    #[serde(default)]
    pub space: Option<String>,
    #[serde(default)]
    pub pane: Option<String>,
    #[serde(default)]
    pub tab: Option<String>,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Tab {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub is_loading: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub favicon_url: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(serialize_bounds(__S: rkyv::ser::Writer + rkyv::ser::Allocator))]
#[rkyv(deserialize_bounds(__D::Error: rkyv::rancor::Source))]
#[rkyv(bytecheck(bounds(__C: rkyv::validation::ArchiveContext)))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayoutNode {
    Split {
        #[serde(default)]
        id: Option<String>,
        direction: SplitDirection,
        #[serde(default)]
        flex_weights: Vec<f32>,
        #[rkyv(omit_bounds)]
        children: Vec<LayoutNode>,
    },
    Pane {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        is_zoomed: bool,
        #[serde(default)]
        tabs: Vec<Tab>,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Space {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub is_active: bool,
    pub root: LayoutNode,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct LayoutSnapshot {
    pub spaces: Vec<Space>,
    pub focused: Focus,
}

pub fn format_id(kind: NodeKind, value: u64) -> String {
    match kind {
        NodeKind::Space => format!("space:{value}"),
        NodeKind::Pane => format!("pane:{value}"),
        NodeKind::Split => format!("split:{value}"),
        NodeKind::Tab => format!("tab:{value}"),
    }
}

pub fn parse_id(s: &str) -> Result<(NodeKind, u64), String> {
    let (prefix, rest) = s
        .split_once(':')
        .ok_or_else(|| format!("id missing ':' separator: {s:?}"))?;
    let kind = match prefix {
        "space" => NodeKind::Space,
        "pane" => NodeKind::Pane,
        "split" => NodeKind::Split,
        "tab" => NodeKind::Tab,
        other => return Err(format!("unknown id prefix {other:?} in {s:?}")),
    };
    let value: u64 = rest
        .parse()
        .map_err(|err| format!("id value not u64 in {s:?}: {err}"))?;
    Ok((kind, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_id_round_trips_each_kind() {
        for (kind, value) in [
            (NodeKind::Space, 1_u64),
            (NodeKind::Pane, 42),
            (NodeKind::Split, 17),
            (NodeKind::Tab, 9999),
        ] {
            let formatted = format_id(kind, value);
            let (parsed_kind, parsed_value) = parse_id(&formatted).unwrap();
            assert_eq!(parsed_kind, kind);
            assert_eq!(parsed_value, value);
        }
    }

    #[test]
    fn parse_id_rejects_missing_separator() {
        assert!(parse_id("pane42").is_err());
    }

    #[test]
    fn parse_id_rejects_unknown_prefix() {
        assert!(parse_id("window:1").is_err());
    }

    #[test]
    fn parse_id_rejects_non_numeric_value() {
        assert!(parse_id("pane:abc").is_err());
    }

    #[test]
    fn layout_snapshot_json_round_trip_minimal() {
        let snapshot = LayoutSnapshot {
            spaces: vec![Space {
                id: Some("space:1".into()),
                name: "Work".into(),
                is_active: true,
                root: LayoutNode::Pane {
                    id: Some("pane:2".into()),
                    is_zoomed: false,
                    tabs: vec![],
                },
            }],
            focused: Focus {
                space: Some("space:1".into()),
                pane: Some("pane:2".into()),
                tab: None,
            },
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: LayoutSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, snapshot);
    }

    #[test]
    fn layout_node_json_discriminator_uses_kind_field() {
        let pane = LayoutNode::Pane {
            id: Some("pane:7".into()),
            is_zoomed: true,
            tabs: vec![],
        };
        let json = serde_json::to_value(&pane).unwrap();
        assert_eq!(json["kind"], "pane");
        assert_eq!(json["id"], "pane:7");
        assert_eq!(json["is_zoomed"], true);
    }

    #[test]
    fn split_serializes_with_snake_case_direction() {
        let split = LayoutNode::Split {
            id: None,
            direction: SplitDirection::Column,
            flex_weights: vec![1.0, 2.0],
            children: vec![],
        };
        let json = serde_json::to_value(&split).unwrap();
        assert_eq!(json["kind"], "split");
        assert_eq!(json["direction"], "column");
    }

    #[test]
    fn rkyv_round_trip_preserves_tree() {
        let snapshot = LayoutSnapshot {
            spaces: vec![Space {
                id: Some("space:1".into()),
                name: "S".into(),
                is_active: true,
                root: LayoutNode::Split {
                    id: Some("split:5".into()),
                    direction: SplitDirection::Row,
                    flex_weights: vec![1.0, 1.0],
                    children: vec![
                        LayoutNode::Pane {
                            id: Some("pane:10".into()),
                            is_zoomed: false,
                            tabs: vec![Tab {
                                id: Some("tab:abc".into()),
                                title: "T".into(),
                                url: "https://x".into(),
                                kind: "browser".into(),
                                is_loading: false,
                                favicon_url: String::new(),
                            }],
                        },
                        LayoutNode::Pane {
                            id: Some("pane:11".into()),
                            is_zoomed: false,
                            tabs: vec![],
                        },
                    ],
                },
            }],
            focused: Focus::default(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&snapshot).unwrap();
        let recovered: LayoutSnapshot =
            rkyv::from_bytes::<LayoutSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, snapshot);
    }
}
