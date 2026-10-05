use bevy::prelude::*;
use vmux_ecs::PageMetadata;
use vmux_ecs::page::PagePlacementCatalog;

use crate::active_pane::ActiveStack;
use crate::pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, Zoomed};
use crate::protocol::{
    Focus, LayoutNode, LayoutSnapshot, NodeKind, SplitDirection, Stack as StackDto, Tab as TabDto,
};
use crate::stack::Stack;
use crate::tab::Tab as LayoutTab;

#[derive(bevy::ecs::system::SystemParam)]
pub struct LayoutSnapshotQuery<'w, 's> {
    tabs: Query<'w, 's, (Entity, &'static LayoutTab, Option<&'static Children>)>,
    splits: Query<'w, 's, (Entity, &'static PaneSplit, Option<&'static Children>), With<Pane>>,
    leaves: Query<'w, 's, (Entity, Option<&'static Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static Children>,
            Option<&'static PageMetadata>,
        ),
        With<Stack>,
    >,
    pane_sizes: Query<'w, 's, &'static PaneSize>,
    zoomed: Query<'w, 's, &'static Zoomed>,
    pages: PagePlacementCatalog<'w, 's>,
}

impl LayoutSnapshotQuery<'_, '_> {
    pub fn build(&self, focused: &ActiveStack, self_stack: Option<Entity>) -> LayoutSnapshot {
        let active_tab = focused.tab;
        let tabs = self
            .tabs
            .iter()
            .map(|(tab_entity, tab, children)| {
                let zoomed_leaf = self.zoomed.get(tab_entity).ok().map(|zoomed| zoomed.leaf);
                let root = children
                    .and_then(|children| children.iter().next())
                    .map(|root| self.node(root, zoomed_leaf, self_stack))
                    .unwrap_or(LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: Vec::new(),
                    });
                TabDto {
                    id: Some(NodeKind::Tab.id(tab_entity.to_bits())),
                    name: tab.name.clone(),
                    is_active: Some(tab_entity) == active_tab,
                    root,
                }
            })
            .collect();

        LayoutSnapshot {
            tabs,
            focused: Focus {
                tab: focused.tab.map(|entity| NodeKind::Tab.id(entity.to_bits())),
                pane: focused
                    .pane
                    .map(|entity| NodeKind::Pane.id(entity.to_bits())),
                stack: focused
                    .stack
                    .map(|entity| NodeKind::Stack.id(entity.to_bits())),
            },
        }
    }

    fn node(
        &self,
        entity: Entity,
        zoomed_leaf: Option<Entity>,
        self_stack: Option<Entity>,
    ) -> LayoutNode {
        if let Ok((split_entity, split, children)) = self.splits.get(entity) {
            let child_entities = children
                .map(|children| children.iter().collect::<Vec<_>>())
                .unwrap_or_default();
            let flex_weights = child_entities
                .iter()
                .map(|child| {
                    self.pane_sizes
                        .get(*child)
                        .map(|size| size.flex_grow)
                        .unwrap_or(1.0)
                })
                .collect();
            let children = child_entities
                .into_iter()
                .map(|child| self.node(child, zoomed_leaf, self_stack))
                .collect();
            return LayoutNode::Split {
                id: Some(NodeKind::Split.id(split_entity.to_bits())),
                direction: match split.direction {
                    PaneSplitDirection::Row => SplitDirection::Row,
                    PaneSplitDirection::Column => SplitDirection::Column,
                },
                flex_weights,
                children,
            };
        }
        if let Ok((leaf_entity, leaf_children)) = self.leaves.get(entity) {
            let stacks = leaf_children
                .map(|children| {
                    children
                        .iter()
                        .filter_map(|child| self.stacks.get(child).ok())
                        .map(|(stack, _, page)| self.stack(stack, page, self_stack))
                        .collect()
                })
                .unwrap_or_default();
            return LayoutNode::Pane {
                id: Some(NodeKind::Pane.id(leaf_entity.to_bits())),
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

    fn stack(
        &self,
        stack_entity: Entity,
        page: Option<&PageMetadata>,
        self_stack: Option<Entity>,
    ) -> StackDto {
        let url = page.map(|page| page.url.clone()).unwrap_or_default();
        StackDto {
            id: Some(NodeKind::Stack.id(stack_entity.to_bits())),
            title: page.map(|page| page.title.clone()).unwrap_or_default(),
            kind: self.pages.resolve(&url).group.to_string(),
            url,
            is_loading: false,
            icon: page.map(|page| page.icon.clone()).unwrap_or_default(),
            is_self: Some(stack_entity) == self_stack,
            process_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::Pane;
    use crate::stack::Stack;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_ecs::page::{PageManifest, PagePlacement};
    use vmux_history::LastActivatedAt;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut().spawn(PageManifest {
            route: "vmux://terminal/",
            url: "vmux://terminal/",
            asset_host: "terminal",
            owns_subtree: true,
            title: "Terminal",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: None,
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: PagePlacement {
                group: "terminal",
                ..PagePlacement::DEFAULT
            },
        });
        app.world_mut().spawn(PageManifest {
            route: "file://",
            url: "vmux://files/",
            asset_host: "files",
            owns_subtree: false,
            title: "Files",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: None,
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: PagePlacement {
                group: "files",
                ..PagePlacement::DEFAULT
            },
        });
        app.world_mut().spawn(ActiveStack::default().local_bundle());
        app
    }

    #[test]
    fn self_stack_is_marked_is_self() {
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
            .spawn((Pane::bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(leaf)))
            .id();
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: "vmux://terminal/x".into(),
            title: String::new(),
            icon: vmux_api::PageIcon::None,
            bg_color: None,
        });

        let snap = app
            .world_mut()
            .run_system_once(
                move |snapshot: LayoutSnapshotQuery,
                      focused: Single<
                    &ActiveStack,
                    With<crate::active_pane::ProfileId>,
                >| { snapshot.build(&focused, Some(stack)) },
            )
            .unwrap();

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
            .spawn((Pane::bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(leaf)))
            .id();
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: "vmux://terminal/123".into(),
            title: String::new(),
            icon: vmux_api::PageIcon::None,
            bg_color: None,
        });

        let snap = app
            .world_mut()
            .run_system_once(
                |snapshot: LayoutSnapshotQuery,
                 focused: Single<&ActiveStack, With<crate::active_pane::ProfileId>>| {
                    snapshot.build(&focused, None)
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
            .spawn((Pane::bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(leaf)))
            .id();
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: "https://example.com".into(),
            title: "Example".into(),
            icon: vmux_api::PageIcon::None,
            bg_color: None,
        });

        let snap = app
            .world_mut()
            .run_system_once(
                |snapshot: LayoutSnapshotQuery,
                 focused: Single<&ActiveStack, With<crate::active_pane::ProfileId>>| {
                    snapshot.build(&focused, None)
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
                |snapshot: LayoutSnapshotQuery,
                 focused: Single<&ActiveStack, With<crate::active_pane::ProfileId>>| {
                    snapshot.build(&focused, None)
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
            let world = app.world_mut();
            let mut query =
                world.query_filtered::<&mut ActiveStack, With<crate::active_pane::ProfileId>>();
            query.single_mut(world).unwrap().tab = Some(tab);
        }

        let snapshot = app
            .world_mut()
            .run_system_once(
                |snapshot: LayoutSnapshotQuery,
                 focused: Single<&ActiveStack, With<crate::active_pane::ProfileId>>| {
                    snapshot.build(&focused, None)
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
            let world = app.world_mut();
            let mut query =
                world.query_filtered::<&mut ActiveStack, With<crate::active_pane::ProfileId>>();
            query.single_mut(world).unwrap().tab = Some(tab);
        }

        let snapshot = app
            .world_mut()
            .run_system_once(
                |snapshot: LayoutSnapshotQuery,
                 focused: Single<&ActiveStack, With<crate::active_pane::ProfileId>>| {
                    snapshot.build(&focused, None)
                },
            )
            .unwrap();

        let root = &snapshot.tabs[0].root;
        let LayoutNode::Split { children, .. } = root else {
            panic!("expected split root")
        };
        let zoomed_flag = children.iter().find_map(|c| match c {
            LayoutNode::Pane { id, is_zoomed, .. } => {
                let expected_id = NodeKind::Pane.id(zoomed_pane.to_bits());
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
                let expected_id = NodeKind::Pane.id(other_pane.to_bits());
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
            .spawn((Pane::bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(leaf)))
            .id();
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: "https://example.com".into(),
            title: "Ex".into(),
            icon: vmux_api::PageIcon::Favicon("https://example.com/icon.png".into()),
            bg_color: None,
        });

        let snap = app
            .world_mut()
            .run_system_once(
                |snapshot: LayoutSnapshotQuery,
                 focused: Single<&ActiveStack, With<crate::active_pane::ProfileId>>| {
                    snapshot.build(&focused, None)
                },
            )
            .unwrap();

        let LayoutNode::Pane { stacks, .. } = &snap.tabs[0].root else {
            panic!("expected pane root");
        };
        assert_eq!(
            stacks[0].icon,
            vmux_api::PageIcon::Favicon("https://example.com/icon.png".into())
        );
    }
}
