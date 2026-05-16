use crate::layout::{
    pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection},
    stack::{FocusedStack, Stack},
    tab::Tab,
};
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_service::protocol::layout::{
    FocusDto, LayoutNodeDto, LayoutSnapshot, NodeKind, SpaceDto, SplitDirectionDto, TabDto,
    format_id,
};

pub(crate) fn build_layout_snapshot(
    spaces_q: &Query<(Entity, &Tab, Option<&Children>)>,
    splits_q: &Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: &Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: &Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    pane_sizes_q: &Query<&PaneSize>,
    terminals: &Query<Entity, With<crate::terminal::Terminal>>,
    focused: &FocusedStack,
) -> LayoutSnapshot {
    let active_space = focused.tab;
    let spaces = spaces_q
        .iter()
        .map(|(space_entity, tab, children)| {
            let root = children
                .and_then(|c| c.iter().next())
                .map(|root_entity| {
                    build_node(
                        root_entity,
                        splits_q,
                        leaves_q,
                        stacks_q,
                        pane_sizes_q,
                        terminals,
                    )
                })
                .unwrap_or(LayoutNodeDto::Pane {
                    id: None,
                    is_zoomed: false,
                    tabs: Vec::new(),
                });
            SpaceDto {
                id: Some(format_id(NodeKind::Space, space_entity.to_bits())),
                name: tab.name.clone(),
                is_active: Some(space_entity) == active_space,
                root,
            }
        })
        .collect();

    LayoutSnapshot {
        spaces,
        focused: FocusDto {
            space: focused.tab.map(|e| format_id(NodeKind::Space, e.to_bits())),
            pane: focused.pane.map(|e| format_id(NodeKind::Pane, e.to_bits())),
            tab: focused.stack.map(|e| format_id(NodeKind::Tab, e.to_bits())),
        },
    }
}

fn build_node(
    entity: Entity,
    splits_q: &Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: &Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: &Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    pane_sizes_q: &Query<&PaneSize>,
    terminals: &Query<Entity, With<crate::terminal::Terminal>>,
) -> LayoutNodeDto {
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
            .map(|child| build_node(child, splits_q, leaves_q, stacks_q, pane_sizes_q, terminals))
            .collect();
        return LayoutNodeDto::Split {
            id: Some(format_id(NodeKind::Split, split_entity.to_bits())),
            direction: match split.direction {
                PaneSplitDirection::Row => SplitDirectionDto::Row,
                PaneSplitDirection::Column => SplitDirectionDto::Column,
            },
            flex_weights,
            children: children_dto,
        };
    }
    if let Ok((leaf_entity, leaf_children)) = leaves_q.get(entity) {
        let tabs = leaf_children
            .map(|c| {
                c.iter()
                    .filter_map(|child| stacks_q.get(child).ok())
                    .map(|(stack_entity, stack_children, page)| {
                        build_tab(stack_entity, stack_children, page, terminals)
                    })
                    .collect()
            })
            .unwrap_or_default();
        return LayoutNodeDto::Pane {
            id: Some(format_id(NodeKind::Pane, leaf_entity.to_bits())),
            is_zoomed: false,
            tabs,
        };
    }
    LayoutNodeDto::Pane {
        id: None,
        is_zoomed: false,
        tabs: Vec::new(),
    }
}

fn build_tab(
    stack_entity: Entity,
    children: Option<&Children>,
    page: Option<&PageMetadata>,
    terminals: &Query<Entity, With<crate::terminal::Terminal>>,
) -> TabDto {
    let kind = if children
        .map(|c| c.iter().any(|child| terminals.contains(child)))
        .unwrap_or(false)
    {
        "terminal"
    } else {
        "browser"
    };
    TabDto {
        id: Some(format_id(NodeKind::Tab, stack_entity.to_bits())),
        title: page.map(|p| p.title.clone()).unwrap_or_default(),
        url: page.map(|p| p.url.clone()).unwrap_or_default(),
        kind: kind.to_string(),
        is_loading: false,
        favicon_url: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::pane::PaneSplitDirection;
    use bevy::ecs::system::RunSystemOnce;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(FocusedStack::default());
        app
    }

    #[test]
    fn empty_world_produces_empty_snapshot() {
        let mut app = make_app();
        let snapshot = app
            .world_mut()
            .run_system_once(
                |spaces: Query<(Entity, &Tab, Option<&Children>)>,
                 splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
                 leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
                 stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
                 terminals: Query<Entity, With<crate::terminal::Terminal>>,
                 pane_sizes: Query<&PaneSize>,
                 focused: Res<FocusedStack>| {
                    build_layout_snapshot(
                        &spaces,
                        &splits,
                        &leaves,
                        &stacks,
                        &pane_sizes,
                        &terminals,
                        &focused,
                    )
                },
            )
            .unwrap();
        assert!(snapshot.spaces.is_empty());
        assert_eq!(snapshot.focused, FocusDto::default());
    }

    #[test]
    fn split_with_two_panes_produces_recursive_node() {
        let mut app = make_app();
        let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(space),
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
            f.tab = Some(space);
        }

        let snapshot = app
            .world_mut()
            .run_system_once(
                |spaces: Query<(Entity, &Tab, Option<&Children>)>,
                 splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
                 leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
                 stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
                 terminals: Query<Entity, With<crate::terminal::Terminal>>,
                 pane_sizes: Query<&PaneSize>,
                 focused: Res<FocusedStack>| {
                    build_layout_snapshot(
                        &spaces,
                        &splits,
                        &leaves,
                        &stacks,
                        &pane_sizes,
                        &terminals,
                        &focused,
                    )
                },
            )
            .unwrap();

        let root = &snapshot.spaces[0].root;
        match root {
            LayoutNodeDto::Split {
                direction,
                flex_weights,
                children,
                ..
            } => {
                assert_eq!(*direction, SplitDirectionDto::Row);
                assert_eq!(flex_weights, &vec![1.0, 2.0]);
                assert_eq!(children.len(), 2);
            }
            other => panic!("expected split, got {other:?}"),
        }
    }
}
