#![allow(dead_code)]

use crate::layout::{
    pane::{PaneSize, PaneSplit, PaneSplitDirection},
    tab::Tab,
};
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_service::protocol::layout::{
    LayoutNodeDto, LayoutSnapshot, SpaceDto, SplitDirectionDto, parse_id,
};

use super::reconcile::ValidationError;

pub fn apply(world: &mut World, snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    super::reconcile::validate(snapshot)?;
    for space in &snapshot.spaces {
        apply_space(world, space);
    }
    Ok(())
}

fn apply_space(world: &mut World, space: &SpaceDto) {
    if let Some(id) = &space.id
        && let Ok((_, value)) = parse_id(id)
    {
        let entity = Entity::from_bits(value);
        apply_structure(world, Some(entity), &space.root);
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
    use vmux_service::protocol::layout::{FocusDto, NodeKind, format_id};

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
}
