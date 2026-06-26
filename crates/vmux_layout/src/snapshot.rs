use bevy::prelude::*;
use vmux_core::PageMetadata;

use crate::pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, Zoomed};
use crate::protocol::format_id;
use crate::protocol::{
    Focus, LayoutNode, LayoutSnapshot, NodeKind, SplitDirection, Stack as StackDto, Tab as TabDto,
};
use crate::stack::{FocusedStack, Stack};
use crate::tab::Tab as LayoutTab;

pub fn build_layout_snapshot(
    tabs_q: &Query<(Entity, &LayoutTab, Option<&Children>)>,
    splits_q: &Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: &Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: &Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    pane_sizes_q: &Query<&PaneSize>,
    zoomed_q: &Query<&Zoomed>,
    focused: &FocusedStack,
    self_stack: Option<Entity>,
) -> LayoutSnapshot {
    let active_tab = focused.tab;
    let tabs = tabs_q
        .iter()
        .map(|(tab_entity, tab, children)| {
            let zoomed_leaf = zoomed_q.get(tab_entity).ok().map(|z| z.leaf);
            let root = children
                .and_then(|c| c.iter().next())
                .map(|root_entity| {
                    build_node(
                        root_entity,
                        splits_q,
                        leaves_q,
                        stacks_q,
                        pane_sizes_q,
                        zoomed_leaf,
                        self_stack,
                    )
                })
                .unwrap_or(LayoutNode::Pane {
                    id: None,
                    is_zoomed: false,
                    stacks: Vec::new(),
                });
            TabDto {
                id: Some(format_id(NodeKind::Tab, tab_entity.to_bits())),
                name: tab.name.clone(),
                is_active: Some(tab_entity) == active_tab,
                root,
            }
        })
        .collect();

    LayoutSnapshot {
        tabs,
        focused: Focus {
            tab: focused.tab.map(|e| format_id(NodeKind::Tab, e.to_bits())),
            pane: focused.pane.map(|e| format_id(NodeKind::Pane, e.to_bits())),
            stack: focused
                .stack
                .map(|e| format_id(NodeKind::Stack, e.to_bits())),
        },
    }
}

fn build_node(
    entity: Entity,
    splits_q: &Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: &Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: &Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    pane_sizes_q: &Query<&PaneSize>,
    zoomed_leaf: Option<Entity>,
    self_stack: Option<Entity>,
) -> LayoutNode {
    if let Ok((split_entity, split, children)) = splits_q.get(entity) {
        let child_entities: Vec<Entity> = children.map(|c| c.iter().collect()).unwrap_or_default();
        let flex_weights = child_entities
            .iter()
            .map(|child| {
                pane_sizes_q
                    .get(*child)
                    .map(|ps| ps.flex_grow)
                    .unwrap_or(1.0)
            })
            .collect();
        let children_dto = child_entities
            .into_iter()
            .map(|child| {
                build_node(
                    child,
                    splits_q,
                    leaves_q,
                    stacks_q,
                    pane_sizes_q,
                    zoomed_leaf,
                    self_stack,
                )
            })
            .collect();
        return LayoutNode::Split {
            id: Some(format_id(NodeKind::Split, split_entity.to_bits())),
            direction: match split.direction {
                PaneSplitDirection::Row => SplitDirection::Row,
                PaneSplitDirection::Column => SplitDirection::Column,
            },
            flex_weights,
            children: children_dto,
        };
    }
    if let Ok((leaf_entity, leaf_children)) = leaves_q.get(entity) {
        let stacks = leaf_children
            .map(|c| {
                c.iter()
                    .filter_map(|child| stacks_q.get(child).ok())
                    .map(|(stack_entity, _stack_children, page)| {
                        build_stack(stack_entity, page, self_stack)
                    })
                    .collect()
            })
            .unwrap_or_default();
        return LayoutNode::Pane {
            id: Some(format_id(NodeKind::Pane, leaf_entity.to_bits())),
            is_zoomed: zoomed_leaf == Some(leaf_entity),
            stacks,
        };
    }
    LayoutNode::Pane {
        id: None,
        is_zoomed: false,
        stacks: Vec::new(),
    }
}

fn stack_kind_for_url(url: &str) -> &'static str {
    if url.starts_with("vmux://terminal/") {
        "terminal"
    } else if url.starts_with("file:") {
        "files"
    } else {
        "browser"
    }
}

fn build_stack(
    stack_entity: Entity,
    page: Option<&PageMetadata>,
    self_stack: Option<Entity>,
) -> StackDto {
    let url = page.map(|p| p.url.clone()).unwrap_or_default();
    StackDto {
        id: Some(format_id(NodeKind::Stack, stack_entity.to_bits())),
        title: page.map(|p| p.title.clone()).unwrap_or_default(),
        kind: stack_kind_for_url(&url).to_string(),
        url,
        is_loading: false,
        icon: page.map(|p| p.icon.clone()).unwrap_or_default(),
        is_self: Some(stack_entity) == self_stack,
        process_id: None,
    }
}

#[cfg(test)]
mod tests {
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
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
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
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
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
                 stacks_q: Query<
                    (Entity, Option<&Children>, Option<&PageMetadata>),
                    With<Stack>,
                >,
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
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
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
                 stacks_q: Query<
                    (Entity, Option<&Children>, Option<&PageMetadata>),
                    With<Stack>,
                >,
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
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
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
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
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
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
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
                 stacks_q: Query<
                    (Entity, Option<&Children>, Option<&PageMetadata>),
                    With<Stack>,
                >,
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
}
