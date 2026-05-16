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
        if let Some(mut tab) = world.get_mut::<Tab>(entity) {
            tab.name = space.name.clone();
        }
    }
    apply_node(world, &space.root);
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
}
