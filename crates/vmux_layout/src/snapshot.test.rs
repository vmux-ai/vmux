use super::*;
use crate::pane::leaf_pane_bundle;
use crate::stack::stack_bundle;
use bevy::ecs::system::RunSystemOnce;
use vmux_history::LastActivatedAt;

fn make_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(FocusedStack::default());
    app
}

#[test]
fn files_url_maps_to_files_kind() {
    assert_eq!(stack_kind_for_url("file:///a/b.rs"), "files");
    assert_eq!(stack_kind_for_url("vmux://terminal/"), "terminal");
    assert_eq!(stack_kind_for_url("https://x.com"), "browser");
}

#[test]
fn self_stack_is_marked_is_self() {
    use bevy::ecs::system::SystemState;
    let mut app = make_app();
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let leaf = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(leaf)))
        .id();
    app.world_mut().entity_mut(stack).insert(PageMetadata {
        url: "vmux://terminal/x".into(),
        title: String::new(),
        icon: vmux_core::PageIcon::None,
        bg_color: None,
    });

    let world = app.world_mut();
    let mut state: SystemState<(
        Query<(Entity, &LayoutTab, Option<&Children>)>,
        Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
        Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
        Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
        Query<&PaneSize>,
        Query<&Zoomed>,
        Res<FocusedStack>,
    )> = SystemState::new(world);
    let (tabs_q, splits_q, leaves_q, stacks_q, pane_sizes_q, zoomed_q, focused) =
        state.get(world).unwrap();
    let snap = build_layout_snapshot(
        &tabs_q,
        &splits_q,
        &leaves_q,
        &stacks_q,
        &pane_sizes_q,
        &zoomed_q,
        &focused,
        Some(stack),
    );

    let LayoutNode::Pane { stacks, .. } = &snap.tabs[0].root else {
        panic!("expected pane");
    };
    assert!(stacks[0].is_self);
}

#[test]
fn terminal_url_classifies_tab_as_terminal() {
    let mut app = make_app();
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let leaf = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(leaf)))
        .id();
    app.world_mut().entity_mut(stack).insert(PageMetadata {
        url: "vmux://terminal/123".into(),
        title: String::new(),
        icon: vmux_core::PageIcon::None,
        bg_color: None,
    });

    let snap = app
        .world_mut()
        .run_system_once(
            |tabs_q: Query<(Entity, &LayoutTab, Option<&Children>)>,
             splits_q: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
             leaves_q: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
             stacks_q: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
             pane_sizes_q: Query<&PaneSize>,
             zoomed_q: Query<&Zoomed>,
             focused: Res<FocusedStack>| {
                build_layout_snapshot(
                    &tabs_q,
                    &splits_q,
                    &leaves_q,
                    &stacks_q,
                    &pane_sizes_q,
                    &zoomed_q,
                    &focused,
                    None,
                )
            },
        )
        .unwrap();

    let LayoutNode::Pane { stacks, .. } = &snap.tabs[0].root else {
        panic!("expected pane root");
    };
    assert_eq!(stacks[0].url, "vmux://terminal/123");
    assert_eq!(stacks[0].kind, "terminal");
}

#[test]
fn browser_url_classifies_tab_as_browser() {
    let mut app = make_app();
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let leaf = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(leaf)))
        .id();
    app.world_mut().entity_mut(stack).insert(PageMetadata {
        url: "https://example.com".into(),
        title: "Example".into(),
        icon: vmux_core::PageIcon::None,
        bg_color: None,
    });

    let snap = app
        .world_mut()
        .run_system_once(
            |tabs_q: Query<(Entity, &LayoutTab, Option<&Children>)>,
             splits_q: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
             leaves_q: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
             stacks_q: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
             pane_sizes_q: Query<&PaneSize>,
             zoomed_q: Query<&Zoomed>,
             focused: Res<FocusedStack>| {
                build_layout_snapshot(
                    &tabs_q,
                    &splits_q,
                    &leaves_q,
                    &stacks_q,
                    &pane_sizes_q,
                    &zoomed_q,
                    &focused,
                    None,
                )
            },
        )
        .unwrap();

    let LayoutNode::Pane { stacks, .. } = &snap.tabs[0].root else {
        panic!("expected pane root");
    };
    assert_eq!(stacks[0].kind, "browser");
    assert_eq!(stacks[0].title, "Example");
}

#[test]
fn empty_world_produces_empty_snapshot() {
    let mut app = make_app();
    let snapshot = app
        .world_mut()
        .run_system_once(
            |tabs: Query<(Entity, &LayoutTab, Option<&Children>)>,
             splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
             leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
             stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
             pane_sizes: Query<&PaneSize>,
             zoomed_q: Query<&Zoomed>,
             focused: Res<FocusedStack>| {
                build_layout_snapshot(
                    &tabs,
                    &splits,
                    &leaves,
                    &stacks,
                    &pane_sizes,
                    &zoomed_q,
                    &focused,
                    None,
                )
            },
        )
        .unwrap();
    assert!(snapshot.tabs.is_empty());
    assert_eq!(snapshot.focused, Focus::default());
}

#[test]
fn split_with_two_panes_produces_recursive_node() {
    let mut app = make_app();
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let _pane_a = app
        .world_mut()
        .spawn((Pane, PaneSize { flex_grow: 1.0 }, ChildOf(split)))
        .id();
    let _pane_b = app
        .world_mut()
        .spawn((Pane, PaneSize { flex_grow: 2.0 }, ChildOf(split)))
        .id();

    {
        let mut f = app.world_mut().resource_mut::<FocusedStack>();
        f.tab = Some(tab);
    }

    let snapshot = app
        .world_mut()
        .run_system_once(
            |tabs: Query<(Entity, &LayoutTab, Option<&Children>)>,
             splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
             leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
             stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
             pane_sizes: Query<&PaneSize>,
             zoomed_q: Query<&Zoomed>,
             focused: Res<FocusedStack>| {
                build_layout_snapshot(
                    &tabs,
                    &splits,
                    &leaves,
                    &stacks,
                    &pane_sizes,
                    &zoomed_q,
                    &focused,
                    None,
                )
            },
        )
        .unwrap();

    let root = &snapshot.tabs[0].root;
    match root {
        LayoutNode::Split {
            direction,
            flex_weights,
            children,
            ..
        } => {
            assert_eq!(*direction, SplitDirection::Row);
            assert_eq!(flex_weights, &vec![1.0, 2.0]);
            assert_eq!(children.len(), 2);
        }
        other => panic!("expected split, got {other:?}"),
    }
}

#[test]
fn zoomed_pane_reports_is_zoomed_true() {
    let mut app = make_app();
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let zoomed_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
    let other_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();

    app.world_mut().entity_mut(tab).insert(Zoomed {
        leaf: zoomed_pane,
        hidden: vec![other_pane],
    });

    {
        let mut f = app.world_mut().resource_mut::<FocusedStack>();
        f.tab = Some(tab);
    }

    let snapshot = app
        .world_mut()
        .run_system_once(
            |tabs: Query<(Entity, &LayoutTab, Option<&Children>)>,
             splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
             leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
             stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
             pane_sizes: Query<&PaneSize>,
             zoomed_q: Query<&Zoomed>,
             focused: Res<FocusedStack>| {
                build_layout_snapshot(
                    &tabs,
                    &splits,
                    &leaves,
                    &stacks,
                    &pane_sizes,
                    &zoomed_q,
                    &focused,
                    None,
                )
            },
        )
        .unwrap();

    let root = &snapshot.tabs[0].root;
    let LayoutNode::Split { children, .. } = root else {
        panic!("expected split root")
    };
    let zoomed_flag = children.iter().find_map(|c| match c {
        LayoutNode::Pane { id, is_zoomed, .. } => {
            let expected_id = format_id(NodeKind::Pane, zoomed_pane.to_bits());
            if id.as_deref() == Some(expected_id.as_str()) {
                Some(*is_zoomed)
            } else {
                None
            }
        }
        _ => None,
    });
    assert_eq!(zoomed_flag, Some(true));

    let other_flag = children.iter().find_map(|c| match c {
        LayoutNode::Pane { id, is_zoomed, .. } => {
            let expected_id = format_id(NodeKind::Pane, other_pane.to_bits());
            if id.as_deref() == Some(expected_id.as_str()) {
                Some(*is_zoomed)
            } else {
                None
            }
        }
        _ => None,
    });
    assert_eq!(other_flag, Some(false));
}

#[test]
fn favicon_url_propagated_from_page_metadata() {
    let mut app = make_app();
    let tab = app
        .world_mut()
        .spawn(LayoutTab {
            name: "S".into(),
            startup_dir: None,
        })
        .id();
    let leaf = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(leaf)))
        .id();
    app.world_mut().entity_mut(stack).insert(PageMetadata {
        url: "https://example.com".into(),
        title: "Ex".into(),
        icon: vmux_core::PageIcon::Favicon("https://example.com/icon.png".into()),
        bg_color: None,
    });

    let snap = app
        .world_mut()
        .run_system_once(
            |tabs_q: Query<(Entity, &LayoutTab, Option<&Children>)>,
             splits_q: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
             leaves_q: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
             stacks_q: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
             pane_sizes_q: Query<&PaneSize>,
             zoomed_q: Query<&Zoomed>,
             focused: Res<FocusedStack>| {
                build_layout_snapshot(
                    &tabs_q,
                    &splits_q,
                    &leaves_q,
                    &stacks_q,
                    &pane_sizes_q,
                    &zoomed_q,
                    &focused,
                    None,
                )
            },
        )
        .unwrap();

    let LayoutNode::Pane { stacks, .. } = &snap.tabs[0].root else {
        panic!("expected pane root");
    };
    assert_eq!(
        stacks[0].icon,
        vmux_core::PageIcon::Favicon("https://example.com/icon.png".into())
    );
}
