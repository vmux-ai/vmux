use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use moonshine_save::prelude::*;
use vmux_api::open_target::PaneDirection;
use vmux_ecs::host::persistence::PersistenceAppExt;
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use super::{identity::SpawnSeq, resize::PaneSize};

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
}

#[derive(SystemParam)]
pub(crate) struct PaneHierarchy<'w, 's> {
    pub(crate) children: Query<'w, 's, &'static Children, With<Pane>>,
    pub(crate) leaves: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    parents: Query<'w, 's, &'static ChildOf>,
    splits: Query<'w, 's, &'static PaneSplit>,
}

impl PaneHierarchy<'_, '_> {
    pub(crate) fn first_leaf(&self, entity: Entity) -> Entity {
        if self.leaves.contains(entity) {
            return entity;
        }
        if let Ok(children) = self.children.get(entity) {
            for child in children.iter() {
                if self.leaves.contains(child) {
                    return child;
                }
                let found = self.first_leaf(child);
                if found != child || self.leaves.contains(found) {
                    return found;
                }
            }
        }
        entity
    }

    pub(crate) fn sibling(&self, active: Entity, direction: PaneDirection) -> Option<Entity> {
        let target_split = PaneSplitDirection::from(direction);
        let after = matches!(direction, PaneDirection::Right | PaneDirection::Bottom);
        let mut current = active;
        for _ in 0..20 {
            let parent = self.parents.get(current).ok()?.parent();
            let Ok(split) = self.splits.get(parent) else {
                current = parent;
                continue;
            };
            if split.direction != target_split {
                current = parent;
                continue;
            }
            let Ok(children) = self.children.get(parent) else {
                current = parent;
                continue;
            };
            let siblings = children.iter().collect::<Vec<_>>();
            let Some(index) = siblings.iter().position(|entity| *entity == current) else {
                current = parent;
                continue;
            };
            let sibling = if after {
                siblings.get(index + 1).copied()
            } else {
                index
                    .checked_sub(1)
                    .and_then(|index| siblings.get(index).copied())
            }?;
            return Some(self.first_leaf(sibling));
        }
        None
    }
}

#[derive(SystemParam)]
pub struct PaneTree<'w, 's> {
    pub(super) commands: Commands<'w, 's>,
}

impl PaneTree<'_, '_> {
    pub fn set_spawn_sequence(&mut self, pane: Entity, sequence: SpawnSeq) {
        self.commands.entity(pane).insert(sequence);
    }

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
    pub(super) fn flex_direction(self) -> FlexDirection {
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
