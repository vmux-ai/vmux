#![allow(dead_code)]

use std::collections::HashSet;

use crate::layout::{
    pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection},
    stack::Stack,
    tab::Tab,
};
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_layout::LayoutSpawnRequest;
use vmux_service::protocol::layout::{
    FocusDto, LayoutNodeDto, LayoutSnapshot, NodeKind, SpaceDto, SplitDirectionDto, TabDto,
    format_id, parse_id,
};

use super::reconcile::ValidationError;

pub fn apply(world: &mut World, snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    let existing = collect_existing_ids(world);
    apply_with_existing(world, snapshot, &existing)
}

pub fn apply_with_existing(
    world: &mut World,
    snapshot: &LayoutSnapshot,
    existing: &HashSet<String>,
) -> Result<(), ValidationError> {
    let plan = super::reconcile::plan_diff(snapshot, existing)?;

    // Pointer-keyed map lets future passes look up entities spawned for id-less nodes.
    let mut new_entities: std::collections::HashMap<*const LayoutNodeDto, Entity> =
        std::collections::HashMap::new();
    for space in &snapshot.spaces {
        let space_entity = match &space.id {
            Some(id) => match parse_id(id) {
                Ok((_, value)) => Entity::from_bits(value),
                Err(_) => continue,
            },
            None => world
                .spawn(Tab {
                    name: space.name.clone(),
                })
                .id(),
        };
        create_descendants(world, space_entity, &space.root, &mut new_entities);
    }

    for space in &snapshot.spaces {
        if let Some(id) = &space.id
            && let Ok((_, value)) = parse_id(id)
        {
            let space_entity = Entity::from_bits(value);
            apply_structure(world, Some(space_entity), &space.root);
        }
    }
    for space in &snapshot.spaces {
        apply_space(world, space);
    }
    for id in &plan.closes {
        apply_close(world, id);
    }
    apply_focus(world, &snapshot.focused);
    Ok(())
}

fn create_descendants(
    world: &mut World,
    parent: Entity,
    node: &LayoutNodeDto,
    new_entities: &mut std::collections::HashMap<*const LayoutNodeDto, Entity>,
) {
    let node_entity = match node {
        LayoutNodeDto::Split { id, direction, .. } => match id {
            Some(id_str) => match parse_id(id_str) {
                Ok((_, v)) => Entity::from_bits(v),
                Err(_) => return,
            },
            None => {
                let entity = spawn_split(world, parent, *direction);
                new_entities.insert(node as *const _, entity);
                entity
            }
        },
        LayoutNodeDto::Pane { id, .. } => match id {
            Some(id_str) => match parse_id(id_str) {
                Ok((_, v)) => Entity::from_bits(v),
                Err(_) => return,
            },
            None => {
                let entity = spawn_leaf_pane(world, parent);
                new_entities.insert(node as *const _, entity);
                entity
            }
        },
    };

    match node {
        LayoutNodeDto::Split { children, .. } => {
            for c in children {
                create_descendants(world, node_entity, c, new_entities);
            }
        }
        LayoutNodeDto::Pane { tabs, .. } => {
            for t in tabs {
                if t.id.is_none() {
                    spawn_tab(world, node_entity, t);
                }
            }
        }
    }
}

fn spawn_split(world: &mut World, parent: Entity, direction: SplitDirectionDto) -> Entity {
    let pane_split_dir = match direction {
        SplitDirectionDto::Row => PaneSplitDirection::Row,
        SplitDirectionDto::Column => PaneSplitDirection::Column,
    };
    world
        .spawn((
            Pane,
            PaneSplit {
                direction: pane_split_dir,
            },
            PaneSize::default(),
            ChildOf(parent),
        ))
        .id()
}

fn spawn_leaf_pane(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((Pane, PaneSize::default(), ChildOf(parent)))
        .id()
}

fn spawn_tab(world: &mut World, pane: Entity, tab: &TabDto) {
    let stack = world.spawn((Stack::default(), ChildOf(pane))).id();
    match tab.kind.as_str() {
        "terminal" => {
            world
                .resource_mut::<Messages<LayoutSpawnRequest>>()
                .write(LayoutSpawnRequest::Terminal { stack });
        }
        _ => {
            world.resource_mut::<Messages<LayoutSpawnRequest>>().write(
                LayoutSpawnRequest::OpenUrl {
                    stack,
                    url: tab.url.clone(),
                },
            );
        }
    }
}

fn apply_close(world: &mut World, id: &str) {
    let Ok((_kind, value)) = parse_id(id) else {
        return;
    };
    let entity = Entity::from_bits(value);
    // TODO: integrate vmux_layout close helpers (PaneCommand::Close paths) so process
    // shutdown and side-sheet sync happen. For v1, brute-force despawn is acceptable.
    if let Ok(entity_ref) = world.get_entity_mut(entity) {
        entity_ref.despawn();
    }
}

fn collect_existing_ids(world: &mut World) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut q_space = world.query_filtered::<Entity, With<Tab>>();
    for e in q_space.iter(world) {
        out.insert(format_id(NodeKind::Space, e.to_bits()));
    }
    let mut q_split = world.query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>();
    for e in q_split.iter(world) {
        out.insert(format_id(NodeKind::Split, e.to_bits()));
    }
    let mut q_pane = world.query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>();
    for e in q_pane.iter(world) {
        out.insert(format_id(NodeKind::Pane, e.to_bits()));
    }
    let mut q_tab = world.query_filtered::<Entity, With<Stack>>();
    for e in q_tab.iter(world) {
        out.insert(format_id(NodeKind::Tab, e.to_bits()));
    }
    out
}

fn apply_space(world: &mut World, space: &SpaceDto) {
    if let Some(id) = &space.id
        && let Ok((_, value)) = parse_id(id)
    {
        let entity = Entity::from_bits(value);
        if let Some(mut tab) = world.get_mut::<Tab>(entity) {
            tab.name = space.name.clone();
        }
    }
    apply_node(world, &space.root);
}

fn apply_structure(world: &mut World, parent: Option<Entity>, node: &LayoutNodeDto) {
    if let Some(entity) = node_entity(node) {
        if let Some(parent) = parent {
            world.entity_mut(entity).insert(ChildOf(parent));
        }
        match node {
            LayoutNodeDto::Split { children, .. } => {
                for c in children {
                    apply_structure(world, Some(entity), c);
                }
            }
            LayoutNodeDto::Pane { tabs, .. } => {
                for t in tabs {
                    if let Some(tid) = t.id.as_deref()
                        && let Ok((_, value)) = parse_id(tid)
                    {
                        let tab_entity = Entity::from_bits(value);
                        world.entity_mut(tab_entity).insert(ChildOf(entity));
                    }
                }
            }
        }
    } else {
        // Created node — Task 10 handles spawning. Descend so any
        // identified descendants still get reparented under their grandparent.
        match node {
            LayoutNodeDto::Split { children, .. } => {
                for c in children {
                    apply_structure(world, parent, c);
                }
            }
            LayoutNodeDto::Pane { .. } => {}
        }
    }
}

fn apply_node(world: &mut World, node: &LayoutNodeDto) {
    match node {
        LayoutNodeDto::Split {
            id,
            direction,
            flex_weights,
            children,
        } => {
            if let Some(id) = id
                && let Ok((_, value)) = parse_id(id)
            {
                let entity = Entity::from_bits(value);
                if let Some(mut split) = world.get_mut::<PaneSplit>(entity) {
                    split.direction = match direction {
                        SplitDirectionDto::Row => PaneSplitDirection::Row,
                        SplitDirectionDto::Column => PaneSplitDirection::Column,
                    };
                }
            }
            if !flex_weights.is_empty() && flex_weights.len() == children.len() {
                for (child_dto, weight) in children.iter().zip(flex_weights.iter()) {
                    if let Some(child_entity) = node_entity(child_dto)
                        && let Some(mut size) = world.get_mut::<PaneSize>(child_entity)
                    {
                        size.flex_grow = *weight;
                    }
                }
            }
            for c in children {
                apply_node(world, c);
            }
        }
        LayoutNodeDto::Pane { tabs, .. } => {
            for t in tabs {
                if let Some(tid) = &t.id
                    && let Ok((_, value)) = parse_id(tid)
                {
                    let entity = Entity::from_bits(value);
                    if !t.title.is_empty()
                        && let Some(mut page) = world.get_mut::<PageMetadata>(entity)
                    {
                        page.title = t.title.clone();
                    }
                }
            }
        }
    }
}

fn apply_focus(world: &mut World, focus: &FocusDto) {
    let Some(mut focused) = world.get_resource_mut::<crate::layout::stack::FocusedStack>() else {
        return;
    };
    focused.tab = focus
        .space
        .as_deref()
        .and_then(|id| parse_id(id).ok())
        .map(|(_, v)| Entity::from_bits(v));
    focused.pane = focus
        .pane
        .as_deref()
        .and_then(|id| parse_id(id).ok())
        .map(|(_, v)| Entity::from_bits(v));
    focused.stack = focus
        .tab
        .as_deref()
        .and_then(|id| parse_id(id).ok())
        .map(|(_, v)| Entity::from_bits(v));
}

fn node_entity(node: &LayoutNodeDto) -> Option<Entity> {
    match node {
        LayoutNodeDto::Split { id, .. } | LayoutNodeDto::Pane { id, .. } => id
            .as_deref()
            .and_then(|id| parse_id(id).ok().map(|(_, value)| Entity::from_bits(value))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::pane::{Pane, PaneSplitDirection};
    use vmux_service::protocol::layout::{FocusDto, NodeKind, TabDto, format_id};

    #[test]
    fn updating_split_direction_changes_component() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
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
        let _pane_a = app.world_mut().spawn((Pane, ChildOf(split))).id();
        let _pane_b = app.world_mut().spawn((Pane, ChildOf(split))).id();

        let snap = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some(format_id(NodeKind::Space, space.to_bits())),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Split {
                    id: Some(format_id(NodeKind::Split, split.to_bits())),
                    direction: SplitDirectionDto::Column,
                    flex_weights: vec![],
                    children: vec![],
                },
            }],
            focused: FocusDto::default(),
        };

        apply(app.world_mut(), &snap).unwrap();
        let updated = app.world().get::<PaneSplit>(split).unwrap();
        assert_eq!(updated.direction, PaneSplitDirection::Column);
    }

    #[test]
    fn updating_flex_weights_writes_pane_size_flex_grow() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
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
        let pane_a = app
            .world_mut()
            .spawn((Pane, PaneSize { flex_grow: 1.0 }, ChildOf(split)))
            .id();
        let pane_b = app
            .world_mut()
            .spawn((Pane, PaneSize { flex_grow: 1.0 }, ChildOf(split)))
            .id();

        let snap = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some(format_id(NodeKind::Space, space.to_bits())),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Split {
                    id: Some(format_id(NodeKind::Split, split.to_bits())),
                    direction: SplitDirectionDto::Row,
                    flex_weights: vec![3.0, 1.0],
                    children: vec![
                        LayoutNodeDto::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_a.to_bits())),
                            is_zoomed: false,
                            tabs: vec![],
                        },
                        LayoutNodeDto::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_b.to_bits())),
                            is_zoomed: false,
                            tabs: vec![],
                        },
                    ],
                },
            }],
            focused: FocusDto::default(),
        };

        apply(app.world_mut(), &snap).unwrap();
        assert_eq!(app.world().get::<PaneSize>(pane_a).unwrap().flex_grow, 3.0);
        assert_eq!(app.world().get::<PaneSize>(pane_b).unwrap().flex_grow, 1.0);
    }

    #[test]
    fn moves_pane_to_new_parent() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
        let split_a = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(space),
            ))
            .id();
        let split_b = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(space),
            ))
            .id();
        let moved = app.world_mut().spawn((Pane, ChildOf(split_a))).id();
        let _filler_b = app.world_mut().spawn((Pane, ChildOf(split_b))).id();

        let snap = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some(format_id(NodeKind::Space, space.to_bits())),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Split {
                    id: Some(format_id(NodeKind::Split, split_a.to_bits())),
                    direction: SplitDirectionDto::Row,
                    flex_weights: vec![],
                    children: vec![LayoutNodeDto::Split {
                        id: Some(format_id(NodeKind::Split, split_b.to_bits())),
                        direction: SplitDirectionDto::Row,
                        flex_weights: vec![],
                        children: vec![LayoutNodeDto::Pane {
                            id: Some(format_id(NodeKind::Pane, moved.to_bits())),
                            is_zoomed: false,
                            tabs: vec![],
                        }],
                    }],
                },
            }],
            focused: FocusDto::default(),
        };

        apply(app.world_mut(), &snap).unwrap();
        let parent = app.world().get::<ChildOf>(moved).map(|p| p.parent());
        assert_eq!(parent, Some(split_b));
    }

    #[test]
    fn omitting_pane_from_snapshot_closes_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
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
        let keep = app.world_mut().spawn((Pane, ChildOf(split))).id();
        let drop_me = app.world_mut().spawn((Pane, ChildOf(split))).id();

        let snap = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some(format_id(NodeKind::Space, space.to_bits())),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Split {
                    id: Some(format_id(NodeKind::Split, split.to_bits())),
                    direction: SplitDirectionDto::Row,
                    flex_weights: vec![],
                    children: vec![LayoutNodeDto::Pane {
                        id: Some(format_id(NodeKind::Pane, keep.to_bits())),
                        is_zoomed: false,
                        tabs: vec![],
                    }],
                },
            }],
            focused: FocusDto::default(),
        };

        let existing: HashSet<String> = [
            format_id(NodeKind::Space, space.to_bits()),
            format_id(NodeKind::Split, split.to_bits()),
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
    fn submitting_new_tab_id_none_spawns_stack_entity() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<vmux_layout::LayoutSpawnRequest>();

        let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();

        let snap = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some(format_id(NodeKind::Space, space.to_bits())),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Pane {
                    id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                    is_zoomed: false,
                    tabs: vec![TabDto {
                        id: None,
                        url: "https://example.com".into(),
                        kind: "browser".into(),
                        ..Default::default()
                    }],
                },
            }],
            focused: FocusDto::default(),
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
        app.add_plugins(MinimalPlugins);
        app.add_message::<vmux_layout::LayoutSpawnRequest>();

        let space = app.world_mut().spawn(Tab { name: "S".into() }).id();

        let pane_count_before = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .count();

        let mut new_entities = std::collections::HashMap::new();
        let bad_node = LayoutNodeDto::Pane {
            id: Some("pane:not_a_number".into()),
            is_zoomed: false,
            tabs: vec![TabDto {
                id: None,
                url: "https://example.com".into(),
                kind: "browser".into(),
                ..Default::default()
            }],
        };
        create_descendants(app.world_mut(), space, &bad_node, &mut new_entities);

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
        assert_eq!(stack_count, 0, "tabs under malformed pane must not spawn");
    }

    #[test]
    fn malformed_split_id_skips_subtree_no_orphan_spawn() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<vmux_layout::LayoutSpawnRequest>();

        let space = app.world_mut().spawn(Tab { name: "S".into() }).id();

        let split_count_before = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .count();

        let mut new_entities = std::collections::HashMap::new();
        let bad_node = LayoutNodeDto::Split {
            id: Some("split:garbage".into()),
            direction: SplitDirectionDto::Row,
            flex_weights: vec![],
            children: vec![LayoutNodeDto::Pane {
                id: None,
                is_zoomed: false,
                tabs: vec![],
            }],
        };
        create_descendants(app.world_mut(), space, &bad_node, &mut new_entities);

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
        let pane_a = app.world_mut().spawn((Pane, ChildOf(split))).id();
        let pane_b = app.world_mut().spawn((Pane, ChildOf(split))).id();
        let pane_c = app.world_mut().spawn((Pane, ChildOf(split))).id();

        // Original order: [a, b, c]. Submit [c, a, b].
        let snap = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some(format_id(NodeKind::Space, space.to_bits())),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Split {
                    id: Some(format_id(NodeKind::Split, split.to_bits())),
                    direction: SplitDirectionDto::Row,
                    flex_weights: vec![],
                    children: vec![
                        LayoutNodeDto::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_c.to_bits())),
                            is_zoomed: false,
                            tabs: vec![],
                        },
                        LayoutNodeDto::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_a.to_bits())),
                            is_zoomed: false,
                            tabs: vec![],
                        },
                        LayoutNodeDto::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_b.to_bits())),
                            is_zoomed: false,
                            tabs: vec![],
                        },
                    ],
                },
            }],
            focused: FocusDto::default(),
        };

        apply(app.world_mut(), &snap).unwrap();

        let children = app
            .world()
            .get::<Children>(split)
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
        app.add_plugins(MinimalPlugins);
        app.insert_resource(crate::layout::stack::FocusedStack::default());

        let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane)))
            .id();

        let snap = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some(format_id(NodeKind::Space, space.to_bits())),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Pane {
                    id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                    is_zoomed: false,
                    tabs: vec![TabDto {
                        id: Some(format_id(NodeKind::Tab, stack.to_bits())),
                        ..Default::default()
                    }],
                },
            }],
            focused: FocusDto {
                space: Some(format_id(NodeKind::Space, space.to_bits())),
                pane: Some(format_id(NodeKind::Pane, pane.to_bits())),
                tab: Some(format_id(NodeKind::Tab, stack.to_bits())),
            },
        };

        apply(app.world_mut(), &snap).unwrap();
        let focused = app.world().resource::<crate::layout::stack::FocusedStack>();
        assert_eq!(focused.tab, Some(space));
        assert_eq!(focused.pane, Some(pane));
        assert_eq!(focused.stack, Some(stack));
    }
}
