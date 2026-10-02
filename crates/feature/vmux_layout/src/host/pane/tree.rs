use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use moonshine_save::prelude::*;
use vmux_api::open_target::PaneDirection;
use vmux_ecs::host::persistence::PersistenceAppExt;
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use super::resize::PaneSize;

pub(super) struct TreePlugin;

impl Plugin for TreePlugin {
    fn build(&self, app: &mut App) {
        app.register_persisted::<Pane>()
            .register_persisted::<PaneSplit>()
            .register_type::<PaneSplitDirection>()
            .add_systems(PostUpdate, sync_direction);
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct Pane;

impl Pane {
    pub fn bundle() -> impl Bundle {
        (
            Self,
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

    pub fn split_bundle(direction: PaneSplitDirection) -> impl Bundle {
        let gaps = direction.gaps(crate::event::PANE_GAP_PX);
        (
            Self,
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

    pub(crate) fn first_leaf(
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
                let found = Self::first_leaf(child, pane_children, leaves);
                if found != child || leaves.contains(found) {
                    return found;
                }
            }
        }
        entity
    }
}

#[derive(SystemParam)]
pub struct PaneTree<'w, 's> {
    commands: Commands<'w, 's>,
}

impl PaneTree<'_, '_> {
    pub(crate) fn split_leaf(
        &mut self,
        active: Entity,
        direction: PaneSplitDirection,
        existing_tabs: &[Entity],
        activate_new: bool,
    ) -> Entity {
        self.spawn_split(active, direction, existing_tabs, activate_new)
            .1
    }

    pub fn split_or_extend(
        &mut self,
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
            return self
                .commands
                .spawn((Pane::bundle(), activity, ChildOf(anchor)))
                .id();
        }
        self.split_leaf(anchor, direction, existing_tabs, activate_new)
    }

    pub(crate) fn spawn_split(
        &mut self,
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
        let existing = self
            .commands
            .spawn((Pane::bundle(), LastActivatedAt::now(), ChildOf(active)))
            .id();
        let new = self
            .commands
            .spawn((Pane::bundle(), new_activity, ChildOf(active)))
            .id();
        for tab in existing_tabs {
            self.commands.entity(*tab).insert(ChildOf(existing));
        }
        self.commands
            .entity(active)
            .insert(Pane::split_bundle(direction));
        (existing, new)
    }
}

#[derive(Component, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct PaneSplit {
    pub direction: PaneSplitDirection,
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

fn sync_direction(mut panes: Query<(&PaneSplit, &mut Node), Changed<PaneSplit>>) {
    for (split, mut node) in &mut panes {
        node.flex_direction = split.direction.flex_direction();
        let gaps = split.direction.gaps(crate::event::PANE_GAP_PX);
        node.column_gap = gaps.column_gap;
        node.row_gap = gaps.row_gap;
    }
}

impl From<PaneDirection> for PaneSplitDirection {
    fn from(direction: PaneDirection) -> Self {
        match direction {
            PaneDirection::Left | PaneDirection::Right => Self::Row,
            PaneDirection::Top | PaneDirection::Bottom => Self::Column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{stack::Stack, tab::Tab};
    use bevy::{ecs::relationship::Relationship, ecs::system::RunSystemOnce};

    #[test]
    fn split_leaf_into_two_reparents_tabs_and_splits() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let active = app
            .world_mut()
            .spawn((Pane::bundle(), LastActivatedAt::now()))
            .id();
        let existing = app
            .world_mut()
            .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(active)))
            .id();

        let new = app
            .world_mut()
            .run_system_once(
                move |mut tree: PaneTree,
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
                    tree.split_leaf(active, PaneSplitDirection::Row, &existing_tabs, true)
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
            .spawn((Pane::bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let existing_stack = app
            .world_mut()
            .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(anchor)))
            .id();

        let (first, second) = app
            .world_mut()
            .run_system_once(move |mut tree: PaneTree| {
                let existing = [existing_stack];
                let first =
                    tree.split_or_extend(anchor, PaneSplitDirection::Row, &existing, false, false);
                let second =
                    tree.split_or_extend(anchor, PaneSplitDirection::Row, &existing, false, true);
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
