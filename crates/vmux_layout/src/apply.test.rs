use super::*;
use crate::protocol::{Focus, SplitDirection, Stack as StackDto, Tab as TabDto};
use std::collections::HashSet;

#[test]
fn collect_existing_ids_scoped_to_active_space() {
    let mut world = World::new();
    let space_a = world.spawn((crate::space::Space, vmux_core::Active)).id();
    let space_b = world.spawn(crate::space::Space).id();
    let tab_a = world
        .spawn((crate::tab::Tab::default(), ChildOf(space_a)))
        .id();
    let tab_b = world
        .spawn((crate::tab::Tab::default(), ChildOf(space_b)))
        .id();
    let ids = collect_existing_ids(&mut world);
    assert!(ids.contains(&format_id(NodeKind::Tab, tab_a.to_bits())));
    assert!(!ids.contains(&format_id(NodeKind::Tab, tab_b.to_bits())));
}

fn pane(id: Option<&str>, stacks: Vec<StackDto>) -> LayoutNode {
    LayoutNode::Pane {
        id: id.map(str::to_string),
        is_zoomed: false,
        stacks,
    }
}

fn split(id: Option<&str>, children: Vec<LayoutNode>, weights: Vec<f32>) -> LayoutNode {
    LayoutNode::Split {
        id: id.map(str::to_string),
        direction: SplitDirection::Row,
        flex_weights: weights,
        children,
    }
}

fn snapshot(root: LayoutNode, focus: Focus) -> LayoutSnapshot {
    LayoutSnapshot {
        tabs: vec![TabDto {
            id: Some("tab:1".into()),
            name: "S".into(),
            is_active: true,
            root,
        }],
        focused: focus,
    }
}

#[test]
fn validate_accepts_minimal_existing_layout() {
    let snap = snapshot(
        pane(
            Some("pane:2"),
            vec![StackDto {
                id: Some("stack:3".into()),
                ..Default::default()
            }],
        ),
        Focus {
            tab: Some("tab:1".into()),
            pane: Some("pane:2".into()),
            stack: Some("stack:3".into()),
        },
    );
    assert!(validate(&snap).is_ok());
}

#[test]
fn validate_rejects_duplicate_pane_id() {
    let snap = snapshot(
        split(
            Some("split:1"),
            vec![pane(Some("pane:2"), vec![]), pane(Some("pane:2"), vec![])],
            vec![1.0, 1.0],
        ),
        Focus::default(),
    );
    assert!(matches!(
        validate(&snap),
        Err(ValidationError::DuplicateId(_))
    ));
}

#[test]
fn validate_rejects_new_pane_without_tabs() {
    let snap = snapshot(pane(None, vec![]), Focus::default());
    assert!(matches!(
        validate(&snap),
        Err(ValidationError::NewPaneMissingStacks)
    ));
}

#[test]
fn validate_rejects_new_tab_without_url() {
    let snap = snapshot(
        pane(
            None,
            vec![StackDto {
                id: None,
                url: String::new(),
                kind: "browser".into(),
                ..Default::default()
            }],
        ),
        Focus::default(),
    );
    assert!(matches!(
        validate(&snap),
        Err(ValidationError::NewStackMissingUrl)
    ));
}

#[test]
fn validate_rejects_new_tab_without_kind() {
    let snap = snapshot(
        pane(
            None,
            vec![StackDto {
                id: None,
                url: "https://x".into(),
                kind: String::new(),
                ..Default::default()
            }],
        ),
        Focus::default(),
    );
    assert!(matches!(
        validate(&snap),
        Err(ValidationError::NewStackMissingKind)
    ));
}

#[test]
fn validate_rejects_focus_to_unknown_id() {
    let snap = snapshot(
        pane(
            Some("pane:2"),
            vec![StackDto {
                id: Some("stack:3".into()),
                ..Default::default()
            }],
        ),
        Focus {
            tab: Some("tab:1".into()),
            pane: Some("pane:99".into()),
            stack: None,
        },
    );
    assert!(matches!(
        validate(&snap),
        Err(ValidationError::FocusReferencesUnknownId(_))
    ));
}

#[test]
fn validate_rejects_wrong_kind_in_position() {
    let snap = snapshot(pane(Some("stack:2"), vec![]), Focus::default());
    assert!(matches!(
        validate(&snap),
        Err(ValidationError::WrongKindForPosition { .. })
    ));
}

#[test]
fn validate_rejects_flex_weights_length_mismatch() {
    let snap = snapshot(
        split(
            Some("split:1"),
            vec![pane(
                Some("pane:2"),
                vec![StackDto {
                    id: Some("stack:3".into()),
                    ..Default::default()
                }],
            )],
            vec![1.0, 2.0],
        ),
        Focus::default(),
    );
    assert!(matches!(
        validate(&snap),
        Err(ValidationError::FlexWeightsLengthMismatch { .. })
    ));
}

#[test]
fn plan_marks_existing_ids_as_matches() {
    let snap = snapshot(
        pane(
            Some("pane:2"),
            vec![StackDto {
                id: Some("stack:3".into()),
                ..Default::default()
            }],
        ),
        Focus {
            tab: Some("tab:1".into()),
            pane: Some("pane:2".into()),
            stack: Some("stack:3".into()),
        },
    );
    let existing: HashSet<String> = ["tab:1", "pane:2", "stack:3"]
        .into_iter()
        .map(String::from)
        .collect();
    let plan = plan_diff(&snap, &existing).unwrap();
    assert!(plan.actions_by_id.contains_key("pane:2"));
    assert!(plan.actions_by_id.contains_key("stack:3"));
    assert!(plan.closes.is_empty());
}

#[test]
fn plan_lists_unreferenced_ids_for_close() {
    let snap = snapshot(
        pane(
            Some("pane:2"),
            vec![StackDto {
                id: Some("stack:3".into()),
                ..Default::default()
            }],
        ),
        Focus {
            tab: Some("tab:1".into()),
            pane: Some("pane:2".into()),
            stack: Some("stack:3".into()),
        },
    );
    let existing: HashSet<String> = ["tab:1", "pane:2", "stack:3", "stack:4"]
        .into_iter()
        .map(String::from)
        .collect();
    let plan = plan_diff(&snap, &existing).unwrap();
    assert_eq!(plan.closes, vec!["stack:4".to_string()]);
}

#[test]
fn plan_treats_id_omission_as_create() {
    let snap = snapshot(
        pane(
            None,
            vec![StackDto {
                id: None,
                url: "https://x".into(),
                kind: "browser".into(),
                ..Default::default()
            }],
        ),
        Focus {
            tab: Some("tab:1".into()),
            pane: None,
            stack: None,
        },
    );
    let existing: HashSet<String> = ["tab:1"].into_iter().map(String::from).collect();
    let plan = plan_diff(&snap, &existing).unwrap();
    assert!(plan.closes.is_empty());
    assert_eq!(plan.actions_by_id.len(), 1);
}

#[test]
fn plan_rejects_referenced_tab_id_not_in_existing() {
    let snap = snapshot(
        pane(
            Some("pane:2"),
            vec![StackDto {
                id: Some("stack:99".into()),
                ..Default::default()
            }],
        ),
        Focus {
            tab: Some("tab:1".into()),
            pane: Some("pane:2".into()),
            stack: Some("stack:99".into()),
        },
    );
    let existing: HashSet<String> = ["tab:1", "pane:2"].into_iter().map(String::from).collect();
    match plan_diff(&snap, &existing) {
        Err(ValidationError::MissingReferencedEntity(ids)) => {
            assert!(
                ids.contains(&"stack:99".to_string()),
                "expected stale tab:99 in error, got {ids:?}"
            );
        }
        other => panic!("expected MissingReferencedEntity, got {other:?}"),
    }
}

#[test]
fn plan_rejects_referenced_pane_id_not_in_existing() {
    let snap = snapshot(pane(Some("pane:42"), vec![]), Focus::default());
    let existing: HashSet<String> = ["tab:1"].into_iter().map(String::from).collect();
    assert!(matches!(
        plan_diff(&snap, &existing),
        Err(ValidationError::MissingReferencedEntity(_))
    ));
}

use crate::pane::{Pane, PaneSplitDirection};
use crate::tab::Tab as LayoutTab;

#[test]
fn updating_split_direction_changes_component() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let split_e = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let _pane_a = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
    let _pane_b = app.world_mut().spawn((Pane, ChildOf(split_e))).id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Split {
                id: Some(format_id(NodeKind::Split, split_e.to_bits())),
                direction: proto::SplitDirection::Column,
                flex_weights: vec![],
                children: vec![],
            },
        }],
        focused: proto::Focus::default(),
    };

    apply(app.world_mut(), &snap).unwrap();
    let updated = app.world().get::<PaneSplit>(split_e).unwrap();
    assert_eq!(updated.direction, PaneSplitDirection::Column);
}

#[test]
fn updating_flex_weights_writes_pane_size_flex_grow() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let split_e = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let pane_a = app
        .world_mut()
        .spawn((Pane, PaneSize { flex_grow: 1.0 }, ChildOf(split_e)))
        .id();
    let pane_b = app
        .world_mut()
        .spawn((Pane, PaneSize { flex_grow: 1.0 }, ChildOf(split_e)))
        .id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Split {
                id: Some(format_id(NodeKind::Split, split_e.to_bits())),
                direction: proto::SplitDirection::Row,
                flex_weights: vec![3.0, 1.0],
                children: vec![
                    proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, pane_a.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                    proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, pane_b.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                ],
            },
        }],
        focused: proto::Focus::default(),
    };

    apply(app.world_mut(), &snap).unwrap();
    assert_eq!(app.world().get::<PaneSize>(pane_a).unwrap().flex_grow, 3.0);
    assert_eq!(app.world().get::<PaneSize>(pane_b).unwrap().flex_grow, 1.0);
}

#[test]
fn moves_pane_to_new_parent() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let split_a = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let split_b = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let moved = app.world_mut().spawn((Pane, ChildOf(split_a))).id();
    let _filler_b = app.world_mut().spawn((Pane, ChildOf(split_b))).id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Split {
                id: Some(format_id(NodeKind::Split, split_a.to_bits())),
                direction: proto::SplitDirection::Row,
                flex_weights: vec![],
                children: vec![proto::LayoutNode::Split {
                    id: Some(format_id(NodeKind::Split, split_b.to_bits())),
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, moved.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    }],
                }],
            },
        }],
        focused: proto::Focus::default(),
    };

    apply(app.world_mut(), &snap).unwrap();
    let parent = app.world().get::<ChildOf>(moved).map(|p| p.parent());
    assert_eq!(parent, Some(split_b));
}

#[test]
fn moves_stack_to_new_tab_reparents_it() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let stack = app.world_mut().spawn(ChildOf(pane)).id();

    // Move the existing stack out of its pane into a brand-new tab (id: None).
    let snap = LayoutSnapshot {
        tabs: vec![
            proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: false,
                root: proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                    is_zoomed: false,
                    stacks: vec![],
                },
            },
            proto::Tab {
                id: None,
                name: "YouTube".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: None,
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                        ..Default::default()
                    }],
                },
            },
        ],
        focused: proto::Focus::default(),
    };
    let existing: std::collections::HashSet<String> = [
        format_id(NodeKind::Tab, tab.to_bits()),
        format_id(NodeKind::Pane, pane.to_bits()),
        format_id(NodeKind::Stack, stack.to_bits()),
    ]
    .into_iter()
    .collect();

    apply_with_existing(app.world_mut(), &snap, &existing).unwrap();

    let s_parent = app
        .world()
        .get::<ChildOf>(stack)
        .map(|p| p.parent())
        .expect("moved stack still has a parent");
    assert_ne!(
        s_parent, pane,
        "stack should be reparented out of the original pane"
    );
    let s_grandparent = app.world().get::<ChildOf>(s_parent).map(|p| p.parent());
    assert!(
        s_grandparent.is_some() && s_grandparent != Some(tab),
        "moved stack's pane should live under the new tab, not the original"
    );
}

#[test]
fn snapshot_active_tab_becomes_most_recently_activated() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();
    let active_tab = app
        .world_mut()
        .spawn((
            LayoutTab {
                name: "A".into(),
                startup_dir: None,
            },
            LastActivatedAt(1),
        ))
        .id();
    let active_pane = app.world_mut().spawn((Pane, ChildOf(active_tab))).id();

    // Keep the existing tab (is_active) and add a NEW tab that must NOT steal active.
    let snap = LayoutSnapshot {
        tabs: vec![
            proto::Tab {
                id: Some(format_id(NodeKind::Tab, active_tab.to_bits())),
                name: "A".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, active_pane.to_bits())),
                    is_zoomed: false,
                    stacks: vec![],
                },
            },
            proto::Tab {
                id: None,
                name: "New".into(),
                is_active: false,
                root: proto::LayoutNode::Pane {
                    id: None,
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: None,
                        url: "https://example.com".into(),
                        kind: "browser".into(),
                        ..Default::default()
                    }],
                },
            },
        ],
        focused: proto::Focus::default(),
    };
    let existing: std::collections::HashSet<String> = [
        format_id(NodeKind::Tab, active_tab.to_bits()),
        format_id(NodeKind::Pane, active_pane.to_bits()),
    ]
    .into_iter()
    .collect();

    apply_with_existing(app.world_mut(), &snap, &existing).unwrap();

    let mut q = app
        .world_mut()
        .query_filtered::<(Entity, &LastActivatedAt), With<LayoutTab>>();
    let ts: Vec<(Entity, i64)> = q.iter(app.world()).map(|(e, l)| (e, l.0)).collect();
    let active_ts = ts
        .iter()
        .find(|(e, _)| *e == active_tab)
        .map(|(_, t)| *t)
        .expect("active tab has a timestamp");
    let max_other = ts
        .iter()
        .filter(|(e, _)| *e != active_tab)
        .map(|(_, t)| *t)
        .max()
        .expect("a new tab exists");
    assert!(
        active_ts > max_other,
        "is_active tab ({active_ts}) must out-rank other tabs ({max_other})"
    );
}

#[test]
fn new_tab_parented_as_sibling_of_existing() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();
    let main = app.world_mut().spawn_empty().id();
    let tab = app
        .world_mut()
        .spawn((
            LayoutTab {
                name: "A".into(),
                startup_dir: None,
            },
            ChildOf(main),
        ))
        .id();
    let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();

    let snap = LayoutSnapshot {
        tabs: vec![
            proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "A".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                    is_zoomed: false,
                    stacks: vec![],
                },
            },
            proto::Tab {
                id: None,
                name: "New".into(),
                is_active: false,
                root: proto::LayoutNode::Pane {
                    id: None,
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: None,
                        url: "https://example.com".into(),
                        kind: "browser".into(),
                        ..Default::default()
                    }],
                },
            },
        ],
        focused: proto::Focus::default(),
    };
    let existing: std::collections::HashSet<String> = [
        format_id(NodeKind::Tab, tab.to_bits()),
        format_id(NodeKind::Pane, pane.to_bits()),
    ]
    .into_iter()
    .collect();

    apply_with_existing(app.world_mut(), &snap, &existing).unwrap();

    let mut q = app
        .world_mut()
        .query_filtered::<(Entity, Option<&ChildOf>), With<LayoutTab>>();
    let parent = q
        .iter(app.world())
        .find(|(e, _)| *e != tab)
        .and_then(|(_, c)| c.map(|c| c.parent()))
        .expect("new tab exists with a parent");
    assert_eq!(
        parent, main,
        "new tab should be a sibling of the existing tab (same parent)"
    );
}

#[test]
fn omitting_pane_from_snapshot_closes_it() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let split_e = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let keep = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
    let drop_me = app.world_mut().spawn((Pane, ChildOf(split_e))).id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Split {
                id: Some(format_id(NodeKind::Split, split_e.to_bits())),
                direction: proto::SplitDirection::Row,
                flex_weights: vec![],
                children: vec![proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, keep.to_bits())),
                    is_zoomed: false,
                    stacks: vec![],
                }],
            },
        }],
        focused: proto::Focus::default(),
    };

    let existing: std::collections::HashSet<String> = [
        format_id(NodeKind::Tab, tab.to_bits()),
        format_id(NodeKind::Split, split_e.to_bits()),
        format_id(NodeKind::Pane, keep.to_bits()),
        format_id(NodeKind::Pane, drop_me.to_bits()),
    ]
    .into_iter()
    .collect();

    apply_with_existing(app.world_mut(), &snap, &existing).unwrap();
    assert!(
        app.world().get_entity(drop_me).is_err(),
        "drop_me should be despawned"
    );
    assert!(app.world().get_entity(keep).is_ok(), "keep should survive");
}

#[test]
fn apply_returns_error_for_stale_tab_id_does_not_panic() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let dead_stack = app
        .world_mut()
        .spawn((Stack::default(), ChildOf(pane_e)))
        .id();
    app.world_mut().entity_mut(dead_stack).despawn();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Pane {
                id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                is_zoomed: false,
                stacks: vec![proto::Stack {
                    id: Some(format_id(NodeKind::Stack, dead_stack.to_bits())),
                    ..Default::default()
                }],
            },
        }],
        focused: proto::Focus::default(),
    };

    let result = apply(app.world_mut(), &snap);
    assert!(
        matches!(result, Err(ValidationError::MissingReferencedEntity(_))),
        "expected MissingReferencedEntity, got {result:?}"
    );
}

#[test]
fn submitting_new_tab_id_none_spawns_stack_entity() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Pane {
                id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                is_zoomed: false,
                stacks: vec![proto::Stack {
                    id: None,
                    url: "https://example.com".into(),
                    kind: "browser".into(),
                    ..Default::default()
                }],
            },
        }],
        focused: proto::Focus::default(),
    };

    apply(app.world_mut(), &snap).unwrap();

    let stack_count = app
        .world_mut()
        .query_filtered::<Entity, With<Stack>>()
        .iter(app.world())
        .count();
    assert_eq!(stack_count, 1, "one new Stack entity should be spawned");
}

#[test]
fn malformed_pane_id_skips_subtree_no_orphan_spawn() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();

    let pane_count_before = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
        .iter(app.world())
        .count();

    let mut new_entities = std::collections::HashMap::new();
    let bad_node = proto::LayoutNode::Pane {
        id: Some("pane:not_a_number".into()),
        is_zoomed: false,
        stacks: vec![proto::Stack {
            id: None,
            url: "https://example.com".into(),
            kind: "browser".into(),
            ..Default::default()
        }],
    };
    materialize_descendants(app.world_mut(), tab, &bad_node, &mut new_entities);

    let pane_count_after = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
        .iter(app.world())
        .count();
    assert_eq!(
        pane_count_before, pane_count_after,
        "malformed id must not spawn orphan pane"
    );

    let stack_count = app
        .world_mut()
        .query_filtered::<Entity, With<Stack>>()
        .iter(app.world())
        .count();
    assert_eq!(stack_count, 0, "stacks under malformed pane must not spawn");
}

#[test]
fn malformed_split_id_skips_subtree_no_orphan_spawn() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();

    let split_count_before = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
        .iter(app.world())
        .count();

    let mut new_entities = std::collections::HashMap::new();
    let bad_node = proto::LayoutNode::Split {
        id: Some("split:garbage".into()),
        direction: proto::SplitDirection::Row,
        flex_weights: vec![],
        children: vec![proto::LayoutNode::Pane {
            id: None,
            is_zoomed: false,
            stacks: vec![],
        }],
    };
    materialize_descendants(app.world_mut(), tab, &bad_node, &mut new_entities);

    let split_count_after = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
        .iter(app.world())
        .count();
    assert_eq!(
        split_count_before, split_count_after,
        "malformed id must not spawn orphan split"
    );

    let pane_count = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
        .iter(app.world())
        .count();
    assert_eq!(
        pane_count, 0,
        "children under malformed split must not spawn"
    );
}

#[test]
fn reordering_split_children_swaps_panes() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let split_e = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let pane_a = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
    let pane_b = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
    let pane_c = app.world_mut().spawn((Pane, ChildOf(split_e))).id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Split {
                id: Some(format_id(NodeKind::Split, split_e.to_bits())),
                direction: proto::SplitDirection::Row,
                flex_weights: vec![],
                children: vec![
                    proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, pane_c.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                    proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, pane_a.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                    proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, pane_b.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                ],
            },
        }],
        focused: proto::Focus::default(),
    };

    apply(app.world_mut(), &snap).unwrap();

    let children = app
        .world()
        .get::<Children>(split_e)
        .expect("split has Children");
    let order: Vec<Entity> = children.iter().collect();
    assert_eq!(
        order,
        vec![pane_c, pane_a, pane_b],
        "Children should match submitted order"
    );
}

#[test]
fn focus_change_writes_focused_stack() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(crate::stack::FocusedStack::default());

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), ChildOf(pane_e)))
        .id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Pane {
                id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                is_zoomed: false,
                stacks: vec![proto::Stack {
                    id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                    ..Default::default()
                }],
            },
        }],
        focused: proto::Focus {
            tab: Some(format_id(NodeKind::Tab, tab.to_bits())),
            pane: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
            stack: Some(format_id(NodeKind::Stack, stack.to_bits())),
        },
    };

    apply(app.world_mut(), &snap).unwrap();
    let focused = app.world().resource::<crate::stack::FocusedStack>();
    assert_eq!(focused.tab, Some(tab));
    assert_eq!(focused.pane, Some(pane_e));
    assert_eq!(focused.stack, Some(stack));
}

#[test]
fn apply_focus_preserves_existing_when_dto_fields_omitted() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .insert_resource(crate::stack::FocusedStack::default());

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), ChildOf(pane_e)))
        .id();

    {
        let mut f = app.world_mut().resource_mut::<crate::stack::FocusedStack>();
        f.tab = Some(tab);
        f.pane = Some(pane_e);
        f.stack = Some(stack);
    }

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Pane {
                id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                is_zoomed: false,
                stacks: vec![proto::Stack {
                    id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                    ..Default::default()
                }],
            },
        }],
        focused: proto::Focus::default(),
    };

    apply(app.world_mut(), &snap).unwrap();
    let f = app.world().resource::<crate::stack::FocusedStack>();
    assert_eq!(f.tab, Some(tab), "focused.tab must be preserved");
    assert_eq!(f.pane, Some(pane_e), "focused.pane must be preserved");
    assert_eq!(f.stack, Some(stack), "focused.stack must be preserved");
}

#[test]
fn new_split_inserts_node_with_flex_direction() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Split {
                id: None,
                direction: proto::SplitDirection::Row,
                flex_weights: vec![],
                children: vec![
                    proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                    proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: None,
                            url: "https://example.com".into(),
                            kind: "browser".into(),
                            ..Default::default()
                        }],
                    },
                ],
            },
        }],
        focused: proto::Focus::default(),
    };

    apply(app.world_mut(), &snap).unwrap();

    let split_count = app
        .world_mut()
        .query_filtered::<&Node, (With<Pane>, With<PaneSplit>)>()
        .iter(app.world())
        .filter(|node| node.flex_direction == bevy::ui::FlexDirection::Row)
        .count();
    assert!(
        split_count >= 1,
        "spawn_split should produce a Pane+PaneSplit with Node{{flex_direction: Row}}"
    );
}

#[test]
fn new_split_wraps_existing_pane_without_converting_it() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let existing_pane = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), ChildOf(existing_pane)))
        .id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Split {
                id: None,
                direction: proto::SplitDirection::Row,
                flex_weights: vec![],
                children: vec![
                    proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, existing_pane.to_bits())),
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                            ..Default::default()
                        }],
                    },
                    proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: None,
                            url: "https://example.com".into(),
                            kind: "browser".into(),
                            ..Default::default()
                        }],
                    },
                ],
            },
        }],
        focused: proto::Focus::default(),
    };

    let existing: std::collections::HashSet<String> = [
        format_id(NodeKind::Tab, tab.to_bits()),
        format_id(NodeKind::Pane, existing_pane.to_bits()),
        format_id(NodeKind::Stack, stack.to_bits()),
    ]
    .into_iter()
    .collect();

    apply_with_existing(app.world_mut(), &snap, &existing).unwrap();

    assert!(
        app.world().get::<PaneSplit>(existing_pane).is_none(),
        "existing pane should stay a leaf"
    );

    let splits: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
        .iter(app.world())
        .collect();
    assert_eq!(splits.len(), 1, "exactly one new split entity should exist");
    let new_split = splits[0];

    let node = app.world().get::<Node>(new_split).unwrap();
    assert_eq!(node.flex_direction, bevy::ui::FlexDirection::Row);

    let children: Vec<Entity> = app
        .world()
        .get::<Children>(new_split)
        .expect("split has children")
        .iter()
        .collect();
    assert_eq!(children.len(), 2, "split should have two leaf children");
    assert_eq!(
        children[0], existing_pane,
        "existing pane should be first per submitted order"
    );

    let stack_parent = app.world().get::<ChildOf>(stack).map(|p| p.parent());
    assert_eq!(
        stack_parent,
        Some(existing_pane),
        "existing stack should stay under existing pane"
    );
}

#[test]
fn new_root_split_id_none_reuses_existing_root_split_of_tab() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let existing_root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let existing_leaf = app
        .world_mut()
        .spawn((leaf_pane_bundle(), ChildOf(existing_root)))
        .id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), ChildOf(existing_leaf)))
        .id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Split {
                id: None,
                direction: proto::SplitDirection::Row,
                flex_weights: vec![],
                children: vec![
                    proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, existing_leaf.to_bits())),
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                            ..Default::default()
                        }],
                    },
                    proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: None,
                            url: "https://example.com".into(),
                            kind: "browser".into(),
                            ..Default::default()
                        }],
                    },
                ],
            },
        }],
        focused: proto::Focus::default(),
    };

    let existing: std::collections::HashSet<String> = [
        format_id(NodeKind::Tab, tab.to_bits()),
        format_id(NodeKind::Split, existing_root.to_bits()),
        format_id(NodeKind::Pane, existing_leaf.to_bits()),
        format_id(NodeKind::Stack, stack.to_bits()),
    ]
    .into_iter()
    .collect();

    apply_with_existing(app.world_mut(), &snap, &existing).unwrap();

    let splits: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
        .iter(app.world())
        .collect();
    assert_eq!(
        splits,
        vec![existing_root],
        "should reuse existing root split, not spawn a new one"
    );

    let children: Vec<Entity> = app
        .world()
        .get::<Children>(existing_root)
        .expect("root split has children")
        .iter()
        .collect();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0], existing_leaf);
}

#[test]
fn serve_snapshot_requests_emits_response() {
    use bevy::ecs::message::Messages;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<LayoutSnapshotRequest>()
        .add_message::<LayoutSnapshotResponse>()
        .insert_resource(crate::stack::FocusedStack::default())
        .add_systems(Update, super::serve_snapshot_requests);

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let _ = app
        .world_mut()
        .spawn((leaf_pane_bundle(), ChildOf(tab)))
        .id();

    app.world_mut()
        .resource_mut::<Messages<LayoutSnapshotRequest>>()
        .write(LayoutSnapshotRequest {
            request_id: [7; 16],
            anchor: None,
        });
    app.update();

    let responses = app.world().resource::<Messages<LayoutSnapshotResponse>>();
    let mut cursor = responses.get_cursor();
    let response = cursor
        .read(responses)
        .next()
        .expect("expected one response");
    assert_eq!(response.request_id, [7; 16]);
    assert_eq!(response.snapshot.tabs.len(), 1);
}

#[test]
fn apply_layout_requests_emits_response_with_snapshot() {
    use bevy::ecs::message::Messages;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<LayoutApplyRequest>()
        .add_message::<LayoutApplyResponse>()
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .insert_resource(crate::stack::FocusedStack::default())
        .add_systems(Update, super::apply_layout_requests);

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let pane = app
        .world_mut()
        .spawn((leaf_pane_bundle(), ChildOf(tab)))
        .id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Pane {
                id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                is_zoomed: false,
                stacks: vec![],
            },
        }],
        focused: proto::Focus::default(),
    };

    app.world_mut()
        .resource_mut::<Messages<LayoutApplyRequest>>()
        .write(LayoutApplyRequest {
            request_id: [42; 16],
            snapshot: snap.clone(),
        });
    app.update();

    let responses = app.world().resource::<Messages<LayoutApplyResponse>>();
    let mut cursor = responses.get_cursor();
    let response = cursor
        .read(responses)
        .next()
        .expect("expected one response");
    assert_eq!(response.request_id, [42; 16]);
    assert!(response.result.is_ok(), "apply should succeed");
}

#[test]
fn new_split_preserves_submitted_children_order_with_new_pane_first() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();

    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let existing_pane = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), ChildOf(existing_pane)))
        .id();

    let snap = LayoutSnapshot {
        tabs: vec![proto::Tab {
            id: Some(format_id(NodeKind::Tab, tab.to_bits())),
            name: "S".into(),
            is_active: true,
            root: proto::LayoutNode::Split {
                id: None,
                direction: proto::SplitDirection::Row,
                flex_weights: vec![],
                children: vec![
                    proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: None,
                            url: "https://example.com".into(),
                            kind: "browser".into(),
                            ..Default::default()
                        }],
                    },
                    proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, existing_pane.to_bits())),
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                            ..Default::default()
                        }],
                    },
                ],
            },
        }],
        focused: proto::Focus::default(),
    };

    let existing: std::collections::HashSet<String> = [
        format_id(NodeKind::Tab, tab.to_bits()),
        format_id(NodeKind::Pane, existing_pane.to_bits()),
        format_id(NodeKind::Stack, stack.to_bits()),
    ]
    .into_iter()
    .collect();

    apply_with_existing(app.world_mut(), &snap, &existing).unwrap();

    let splits: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
        .iter(app.world())
        .collect();
    let new_split = splits[0];
    let children: Vec<Entity> = app
        .world()
        .get::<Children>(new_split)
        .expect("split has children")
        .iter()
        .collect();
    assert_eq!(children.len(), 2);
    assert_eq!(
        children[1], existing_pane,
        "existing pane should be second per submitted order"
    );
}
