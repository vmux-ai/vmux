use bevy::prelude::*;
use moonshine_save::prelude::*;
use vmux_api::open_target::PaneDirection;
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use super::resize::{PaneSize, pane_split_gaps};

pub(super) struct TreePlugin;

impl Plugin for TreePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Pane>()
            .register_type::<PaneSplit>()
            .register_type::<PaneSplitDirection>();
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct Pane;

#[derive(Component, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct PaneSplit {
    pub direction: PaneSplitDirection,
}

pub(crate) fn set_split_direction(
    world: &mut World,
    entity: Entity,
    direction: PaneSplitDirection,
) {
    if let Some(mut split) = world.get_mut::<PaneSplit>(entity) {
        split.direction = direction;
    }
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.flex_direction = direction.flex_direction();
        let gaps = pane_split_gaps(direction, crate::event::PANE_GAP_PX);
        node.column_gap = gaps.column_gap;
        node.row_gap = gaps.row_gap;
    }
}

pub(crate) fn spawn_split_from_leaf(
    commands: &mut Commands,
    active: Entity,
    direction: PaneSplitDirection,
    existing_tabs: &[Entity],
    activate_new: bool,
) -> (Entity, Entity) {
    let new_activity = if activate_new {
        LastActivatedAt::now()
    } else {
        LastActivatedAt(0)
    };
    let existing = commands
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(active)))
        .id();
    let new = commands
        .spawn((leaf_pane_bundle(), new_activity, ChildOf(active)))
        .id();
    for tab in existing_tabs {
        commands.entity(*tab).insert(ChildOf(existing));
    }
    commands.entity(active).insert(split_root_bundle(direction));
    (existing, new)
}

#[derive(Reflect, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[type_path = "vmux_desktop::layout::pane"]
pub enum PaneSplitDirection {
    #[default]
    Row,
    Column,
}

impl PaneSplitDirection {
    fn flex_direction(self) -> FlexDirection {
        match self {
            Self::Row => FlexDirection::Row,
            Self::Column => FlexDirection::Column,
        }
    }
}

pub fn leaf_pane_bundle() -> impl Bundle {
    (
        Pane,
        PaneSize::default(),
        Transform::default(),
        Node {
            flex_grow: 1.0,
            flex_basis: Val::Px(0.0),
            align_items: AlignItems::Stretch,
            justify_content: JustifyContent::Stretch,
            ..default()
        },
    )
}

pub fn split_root_bundle(direction: PaneSplitDirection) -> impl Bundle {
    let gaps = pane_split_gaps(direction, crate::event::PANE_GAP_PX);
    (
        Pane,
        PaneSplit { direction },
        PaneSize::default(),
        Transform::default(),
        Visibility::default(),
        Node {
            flex_grow: 1.0,
            flex_direction: direction.flex_direction(),
            column_gap: gaps.column_gap,
            row_gap: gaps.row_gap,
            align_items: AlignItems::Stretch,
            ..default()
        },
    )
}

pub fn first_leaf_descendant(
    entity: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    leaves: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Entity {
    if leaves.contains(entity) {
        return entity;
    }
    if let Ok(children) = pane_children.get(entity) {
        for child in children.iter() {
            if leaves.contains(child) {
                return child;
            }
            let found = first_leaf_descendant(child, pane_children, leaves);
            if found != child || leaves.contains(found) {
                return found;
            }
        }
    }
    entity
}

pub fn direction_to_split(direction: &PaneDirection) -> PaneSplitDirection {
    match direction {
        PaneDirection::Left | PaneDirection::Right => PaneSplitDirection::Row,
        PaneDirection::Top | PaneDirection::Bottom => PaneSplitDirection::Column,
    }
}

pub fn split_leaf_into_two(
    commands: &mut Commands,
    active: Entity,
    direction: PaneSplitDirection,
    existing_tabs: &[Entity],
    activate_new: bool,
) -> Entity {
    spawn_split_from_leaf(commands, active, direction, existing_tabs, activate_new).1
}

pub fn split_or_extend(
    commands: &mut Commands,
    anchor: Entity,
    direction: PaneSplitDirection,
    existing_tabs: &[Entity],
    activate_new: bool,
    already_split: bool,
) -> Entity {
    if already_split {
        let activity = if activate_new {
            LastActivatedAt::now()
        } else {
            LastActivatedAt(0)
        };
        return commands
            .spawn((leaf_pane_bundle(), activity, ChildOf(anchor)))
            .id();
    }
    split_leaf_into_two(commands, anchor, direction, existing_tabs, activate_new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{stack::Stack, stack::stack_bundle, tab::Tab};
    use bevy::{ecs::relationship::Relationship, ecs::system::RunSystemOnce};

    #[test]
    fn split_leaf_into_two_reparents_tabs_and_splits() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let active = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now()))
            .id();
        let existing = app
            .world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(active)))
            .id();

        let new = app
            .world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      children: Query<&Children, With<Pane>>,
                      stacks: Query<Entity, With<Stack>>| {
                    let existing_tabs: Vec<Entity> = children
                        .get(active)
                        .map(|children| {
                            children
                                .iter()
                                .filter(|&entity| stacks.contains(entity))
                                .collect()
                        })
                        .unwrap_or_default();
                    split_leaf_into_two(
                        &mut commands,
                        active,
                        PaneSplitDirection::Row,
                        &existing_tabs,
                        true,
                    )
                },
            )
            .unwrap();

        assert!(app.world().get::<PaneSplit>(active).is_some());
        assert_ne!(app.world().get::<ChildOf>(existing).unwrap().get(), active);
        assert!(app.world().get::<PaneSplit>(new).is_none());
    }

    #[test]
    fn split_or_extend_batched_runs_make_no_empty_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let anchor = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let existing_stack = app
            .world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor)))
            .id();

        let (first, second) = app
            .world_mut()
            .run_system_once(move |mut commands: Commands| {
                let existing = [existing_stack];
                let first = split_or_extend(
                    &mut commands,
                    anchor,
                    PaneSplitDirection::Row,
                    &existing,
                    false,
                    false,
                );
                let second = split_or_extend(
                    &mut commands,
                    anchor,
                    PaneSplitDirection::Row,
                    &existing,
                    false,
                    true,
                );
                (first, second)
            })
            .unwrap();

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(anchor)
            .unwrap()
            .iter()
            .collect();
        assert_eq!(children.len(), 3);
        assert!(children.contains(&first));
        assert!(children.contains(&second));
        let stack_holders = children
            .iter()
            .filter(|&&child| {
                app.world().get::<Children>(child).is_some_and(|children| {
                    children
                        .iter()
                        .any(|entity| app.world().get::<Stack>(entity).is_some())
                })
            })
            .count();
        assert_eq!(stack_holders, 1);
        let empty_leaves = children
            .iter()
            .filter(|&&child| {
                app.world()
                    .get::<Children>(child)
                    .map(|children| children.iter().count())
                    .unwrap_or(0)
                    == 0
            })
            .count();
        assert_eq!(empty_leaves, 2);
    }
}
