use std::collections::HashSet;

use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

use crate::{Pane, PaneSplit, PaneSplitDirection};

#[derive(SystemParam)]
pub struct Hierarchy<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    panes: Query<'w, 's, (), With<Pane>>,
    leaves: Query<'w, 's, (), (With<Pane>, Without<PaneSplit>)>,
    parents: Query<'w, 's, &'static ChildOf>,
    splits: Query<'w, 's, &'static PaneSplit>,
    entities: Query<'w, 's, Entity>,
}

impl Hierarchy<'_, '_> {
    pub fn children(&self, entity: Entity) -> Option<&Children> {
        if !self.panes.contains(entity) {
            return None;
        }
        self.children.get(entity).ok()
    }

    pub fn is_leaf(&self, entity: Entity) -> bool {
        self.leaves.contains(entity)
    }

    pub fn contains(&self, root: Entity, entity: Entity) -> bool {
        if !self.entities.contains(root) || !self.entities.contains(entity) {
            return false;
        }
        let mut found = false;
        let mut current = entity;
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(current) {
                return false;
            }
            found |= current == root;
            let Ok(parent) = self.parents.get(current) else {
                return found;
            };
            current = parent.parent();
        }
    }

    pub fn first_leaf(&self, entity: Entity) -> Entity {
        self.leaf(entity).unwrap_or(entity)
    }

    pub fn first_leaf_in(&self, root: Entity, entity: Entity) -> Option<Entity> {
        if !self.contains(root, entity) {
            return None;
        }
        self.leaf(entity)
    }

    fn leaf(&self, entity: Entity) -> Option<Entity> {
        let mut pending = vec![entity];
        let mut seen = HashSet::new();
        while let Some(entity) = pending.pop() {
            if !seen.insert(entity) || !self.panes.contains(entity) {
                continue;
            }
            if self.leaves.contains(entity) {
                return Some(entity);
            }
            if let Ok(children) = self.children.get(entity) {
                for child in children.iter().rev() {
                    pending.push(child);
                }
            }
        }
        None
    }

    pub fn neighbor_in(
        &self,
        root: Entity,
        active: Entity,
        axis: PaneSplitDirection,
        after: bool,
    ) -> Option<Entity> {
        if !self.contains(root, active) {
            return None;
        }
        self.neighbor(Some(root), active, axis, after)
    }

    fn neighbor(
        &self,
        root: Option<Entity>,
        active: Entity,
        axis: PaneSplitDirection,
        after: bool,
    ) -> Option<Entity> {
        let mut current = active;
        let mut seen = HashSet::new();
        while seen.insert(current) {
            if Some(current) == root {
                return None;
            }
            let parent = self.parents.get(current).ok()?.parent();
            if let Ok(split) = self.splits.get(parent)
                && split.direction == axis
                && let Ok(children) = self.children.get(parent)
            {
                let siblings: Vec<Entity> = children
                    .iter()
                    .filter(|child| self.panes.contains(*child))
                    .collect();
                if let Some(index) = siblings.iter().position(|entity| *entity == current) {
                    let sibling = if after {
                        siblings.get(index + 1).copied()
                    } else {
                        index
                            .checked_sub(1)
                            .and_then(|index| siblings.get(index).copied())
                    }?;
                    return self.leaf(sibling);
                }
            }
            current = parent;
        }
        None
    }

    #[cfg(feature = "contracts")]
    pub fn sibling(
        &self,
        active: Entity,
        direction: vmux_api::open_target::PaneDirection,
    ) -> Option<Entity> {
        let after = matches!(
            direction,
            vmux_api::open_target::PaneDirection::Right
                | vmux_api::open_target::PaneDirection::Bottom
        );
        self.neighbor(None, active, direction.into(), after)
    }
}
