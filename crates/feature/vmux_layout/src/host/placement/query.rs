use std::collections::BTreeSet;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use vmux_ecs::page::{PagePlacement, PagePlacementCatalog, PageSplitAxis};
use vmux_ecs::{EntityTarget, PageMetadata, PageOpenTask};
use vmux_flex::ComputedNode;

use crate::pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, SpawnSeq};
use crate::space::Space;
use crate::stack::Stack;
use crate::tab::Tab;

use super::{CreatedStack, OpenIntent, Routed, Unplaced};

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct Scope {
    tab: Option<Entity>,
    space: Option<Entity>,
    window: Option<Entity>,
}

#[derive(SystemParam)]
pub(crate) struct PlacementQuery<'w, 's> {
    panes: Query<'w, 's, (), With<Pane>>,
    splits: Query<'w, 's, &'static PaneSplit>,
    children: Query<'w, 's, &'static Children>,
    parents: Query<'w, 's, &'static ChildOf>,
    tabs: Query<'w, 's, (), With<Tab>>,
    spaces: Query<'w, 's, (), With<Space>>,
    hosts: Query<'w, 's, &'static HostWindow>,
    windows: Query<'w, 's, (), With<Window>>,
    stacks: Query<'w, 's, (), With<Stack>>,
    unplaced: Query<'w, 's, (), With<Unplaced>>,
    metadata: Query<'w, 's, &'static PageMetadata, With<Stack>>,
    tasks: Query<'w, 's, &'static PageOpenTask>,
    navigations: Query<'w, 's, &'static vmux_ecs::PageNavigation>,
    routed: Query<
        'w,
        's,
        (
            &'static EntityTarget<Stack>,
            &'static OpenIntent,
            Has<CreatedStack>,
        ),
        With<Routed>,
    >,
    sequences: Query<'w, 's, &'static SpawnSeq>,
    nodes: Query<'w, 's, &'static ComputedNode>,
    weights: Query<'w, 's, &'static PaneSize>,
    placements: PagePlacementCatalog<'w, 's>,
}

impl PlacementQuery<'_, '_> {
    pub(super) fn scope(&self, pane: Entity) -> Option<Scope> {
        if !self.panes.contains(pane) {
            return None;
        }
        let mut scope = Scope {
            tab: None,
            space: None,
            window: None,
        };
        let mut current = pane;
        for _ in 0..128 {
            if self.tabs.contains(current) {
                scope.tab = Some(current);
            }
            if self.spaces.contains(current) {
                scope.space = Some(current);
            }
            if let Ok(host) = self.hosts.get(current) {
                scope.window = Some(host.0);
            }
            let Ok(parent) = self.parents.get(current) else {
                return Some(scope);
            };
            current = parent.parent();
        }
        None
    }

    pub(super) fn tab(&self, pane: Entity) -> Option<Entity> {
        self.scope(pane).and_then(|scope| scope.tab)
    }

    pub(super) fn valid(&self, pane: Entity, scope: Scope) -> bool {
        self.scope(pane) == Some(scope)
            && scope
                .window
                .is_none_or(|window| self.windows.contains(window))
    }

    pub(super) fn stack_exists(&self, stack: Entity) -> bool {
        self.stacks.contains(stack)
    }

    pub(super) fn unadmitted(&self, stack: Entity) -> bool {
        self.stacks.contains(stack)
            && self.unplaced.contains(stack)
            && !self.parents.contains(stack)
    }

    pub(super) fn parent(&self, entity: Entity) -> Option<Entity> {
        self.parents.get(entity).ok().map(ChildOf::parent)
    }

    pub(super) fn is_split(&self, pane: Entity) -> bool {
        self.splits.contains(pane)
    }

    pub(super) fn placement(&self, url: &str) -> PagePlacement {
        self.placements.resolve(url)
    }

    pub(super) fn navigation(&self, stack: Entity) -> Option<vmux_ecs::PageOpenId> {
        self.navigations
            .get(stack)
            .ok()
            .map(|navigation| navigation.id())
    }

    pub(super) fn registered(&self, url: &str) -> bool {
        self.placements.registered(url)
    }

    pub(super) fn stack_url(&self, stack: Entity) -> Option<&str> {
        for (target, intent, _) in &self.routed {
            if target.entity() == stack {
                return Some(&intent.url);
            }
        }
        for task in &self.tasks {
            if task.stack == stack {
                return Some(&task.url);
            }
        }
        self.metadata.get(stack).ok().map(|page| page.url.as_str())
    }

    pub(super) fn fresh(&self, stack: Entity) -> bool {
        self.routed
            .iter()
            .any(|(target, _, created)| created && target.entity() == stack)
    }

    pub(super) fn stacks(&self, pane: Entity) -> Vec<Entity> {
        let mut stacks = Vec::new();
        if let Ok(children) = self.children.get(pane) {
            for child in children {
                if self.stacks.contains(*child) && !self.unplaced.contains(*child) {
                    stacks.push(*child);
                }
            }
        }
        stacks
    }

    pub(super) fn leaves(&self, anchor: Entity) -> Vec<Entity> {
        let root = self
            .scope(anchor)
            .and_then(|scope| scope.tab)
            .unwrap_or(anchor);
        let mut leaves = Vec::new();
        let mut pending = vec![root];
        let mut visited = BTreeSet::new();
        while let Some(entity) = pending.pop() {
            if !visited.insert(entity) {
                continue;
            }
            if self.panes.contains(entity) && !self.splits.contains(entity) {
                leaves.push(entity);
            }
            if let Ok(children) = self.children.get(entity) {
                pending.extend(children.iter().rev());
            }
        }
        leaves
    }

    fn groups(&self, pane: Entity) -> BTreeSet<&'static str> {
        let mut groups = BTreeSet::new();
        for stack in self.stacks(pane) {
            if let Some(url) = self.stack_url(stack) {
                groups.insert(self.placements.resolve(url).group);
            }
        }
        groups
    }

    pub(super) fn reuse(&self, anchor: Entity, url: &str, include_pending: bool) -> Option<Entity> {
        let scope = self.scope(anchor)?;
        let root = scope.space.or(scope.tab)?;
        let mut pending = vec![root];
        let mut visited = BTreeSet::new();
        while let Some(entity) = pending.pop() {
            if !visited.insert(entity) {
                continue;
            }
            if self.stacks.contains(entity)
                && !self.unplaced.contains(entity)
                && (include_pending || !self.fresh(entity))
                && self
                    .stack_url(entity)
                    .is_some_and(|current| self.placements.reuses(url, current))
            {
                return Some(entity);
            }
            if let Ok(children) = self.children.get(entity) {
                pending.extend(children.iter().rev());
            }
        }
        None
    }

    fn auxiliary(&self, pane: Entity) -> bool {
        for stack in self.stacks(pane) {
            if let Some(url) = self.stack_url(stack)
                && self.placement(url).auxiliary
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn primary(&self, anchor: Entity) -> Entity {
        let leaves = self.leaves(anchor);
        let mut newest = None;
        for &pane in &leaves {
            if self.auxiliary(pane) {
                continue;
            }
            let sequence = self.sequences.get(pane).map_or(0, |sequence| sequence.0);
            if newest.is_none_or(|(_, old)| sequence > old) {
                newest = Some((pane, sequence));
            }
        }
        newest
            .map(|(pane, _)| pane)
            .or_else(|| leaves.first().copied())
            .unwrap_or(anchor)
    }

    pub(super) fn destination(
        &self,
        anchor: Entity,
        url: &str,
    ) -> (Entity, Option<PaneSplitDirection>) {
        let placement = self.placement(url);
        let leaves = self.leaves(anchor);
        for &pane in &leaves {
            if self.groups(pane).is_empty() {
                return (pane, None);
            }
        }
        let mut same = None;
        for &pane in &leaves {
            let groups = self.groups(pane);
            if groups.len() != 1 || !groups.contains(placement.group) {
                continue;
            }
            let sequence = self.sequences.get(pane).map_or(0, |sequence| sequence.0);
            if same.is_none_or(|(_, old)| sequence > old) {
                same = Some((pane, sequence));
            }
        }
        if let Some((pane, _)) = same {
            return (pane, None);
        }
        if !placement.auxiliary
            && let Some(preference) = placement.split
        {
            let mut valid = true;
            let mut target = None;
            for &pane in &leaves {
                let groups = self.groups(pane);
                valid &= groups
                    .iter()
                    .all(|group| preference.allowed_groups.contains(group));
                if groups.len() == 1 && groups.contains(preference.anchor_group) {
                    let sequence = self.sequences.get(pane).map_or(0, |sequence| sequence.0);
                    if target.is_none_or(|(_, old)| sequence > old) {
                        target = Some((pane, sequence));
                    }
                }
            }
            if valid && let Some((pane, _)) = target {
                let direction = match preference.axis {
                    PageSplitAxis::Row => PaneSplitDirection::Row,
                    PageSplitAxis::Column => PaneSplitDirection::Column,
                };
                return (pane, Some(direction));
            }
        }
        let primary = self.primary(anchor);
        if placement.auxiliary && self.auxiliary(primary) {
            return (anchor, None);
        }
        let size = self.size(primary);
        let axis = if size.x >= size.y {
            PaneSplitDirection::Row
        } else {
            PaneSplitDirection::Column
        };
        (primary, Some(axis))
    }

    pub(super) fn size(&self, pane: Entity) -> Vec2 {
        let mut current = pane;
        let mut scale = Vec2::ONE;
        for _ in 0..128 {
            if let Ok(node) = self.nodes.get(current)
                && node.size != Vec2::ZERO
            {
                return node.size * scale;
            }
            let Some(parent) = self.parent(current) else {
                break;
            };
            if let Ok(split) = self.splits.get(parent) {
                let mut total = 0.0;
                if let Ok(children) = self.children.get(parent) {
                    for child in children {
                        if self.panes.contains(*child) {
                            total += self.weights.get(*child).map_or(1.0, |size| size.flex_grow);
                        }
                    }
                }
                let weight = self.weights.get(current).map_or(1.0, |size| size.flex_grow);
                let factor = if total > 0.0 { weight / total } else { 1.0 };
                match split.direction {
                    PaneSplitDirection::Row => scale.x *= factor,
                    PaneSplitDirection::Column => scale.y *= factor,
                }
            }
            current = parent;
        }
        Vec2::ZERO
    }

    pub(super) fn sibling(
        &self,
        pane: Entity,
        direction: vmux_api::open_target::PaneDirection,
    ) -> Option<Entity> {
        let parent = self.parent(pane)?;
        let split = self.splits.get(parent).ok()?;
        if split.direction != PaneSplitDirection::from(direction) {
            return None;
        }
        let children = self.children.get(parent).ok()?;
        let index = children.iter().position(|entity| entity == pane)?;
        let next = match direction {
            vmux_api::open_target::PaneDirection::Left
            | vmux_api::open_target::PaneDirection::Top => index.checked_sub(1)?,
            vmux_api::open_target::PaneDirection::Right
            | vmux_api::open_target::PaneDirection::Bottom => index + 1,
        };
        children.get(next).copied()
    }
}
