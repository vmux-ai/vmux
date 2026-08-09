use super::*;

#[test]
fn parse_id_round_trips_each_kind() {
    for (kind, value) in [
        (NodeKind::Tab, 1_u64),
        (NodeKind::Pane, 42),
        (NodeKind::Split, 17),
        (NodeKind::Stack, 9999),
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
        tabs: vec![Tab {
            id: Some("tab:1".into()),
            name: "Work".into(),
            is_active: true,
            root: LayoutNode::Pane {
                id: Some("pane:2".into()),
                is_zoomed: false,
                stacks: vec![],
            },
        }],
        focused: Focus {
            tab: Some("tab:1".into()),
            pane: Some("pane:2".into()),
            stack: None,
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
        stacks: vec![],
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
        tabs: vec![Tab {
            id: Some("tab:1".into()),
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
                        stacks: vec![Stack {
                            id: Some("stack:12".into()),
                            title: "T".into(),
                            url: "https://x".into(),
                            kind: "browser".into(),
                            is_loading: false,
                            icon: crate::PageIcon::None,
                            is_self: false,
                            process_id: None,
                        }],
                    },
                    LayoutNode::Pane {
                        id: Some("pane:11".into()),
                        is_zoomed: false,
                        stacks: vec![],
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
