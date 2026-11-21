use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use vmux_ecs::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_flex::prelude::*;
use vmux_history::{CreatedAt, LastActivatedAt};

use crate::TerminalLayoutSpawnRequest;
use crate::pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, Zoomed};
use crate::protocol::{self as proto, LayoutNode};
use crate::stack::Stack;
use crate::tab::Tab;

use super::Reconcile;

impl Reconcile {
    pub(super) fn tab(&self, world: &mut World, tab: &proto::Tab, scope: Option<Entity>) -> Entity {
        let entity = match tab.id.as_deref() {
            Some(id) => Self::parse(id).unwrap().1,
            None => world
                .spawn((Tab::bundle(), CreatedAt::now(), LastActivatedAt(0)))
                .id(),
        };
        if let Some(scope) = scope {
            world.entity_mut(entity).insert(ChildOf(scope));
        }
        world.get_mut::<Tab>(entity).unwrap().name = tab.name.clone();
        entity
    }

    pub(super) fn node(
        &self,
        world: &mut World,
        node: &LayoutNode,
        parent: Entity,
        reuse: Option<Entity>,
    ) -> (Entity, Option<Entity>) {
        match node {
            LayoutNode::Split {
                id,
                direction,
                flex_weights,
                children,
            } => {
                let direction = match direction {
                    proto::SplitDirection::Row => PaneSplitDirection::Row,
                    proto::SplitDirection::Column => PaneSplitDirection::Column,
                };
                let target = id.as_deref().map(|id| Self::parse(id).unwrap().1).or(reuse);
                let entity = self.split(world, target, parent, direction);
                let mut ordered = Vec::new();
                let mut zoomed = None;
                for (index, child) in children.iter().enumerate() {
                    let (target, child_zoomed) = self.node(world, child, entity, None);
                    if let Some(weight) = flex_weights.get(index) {
                        world
                            .entity_mut(target)
                            .insert(PaneSize { flex_grow: *weight });
                        if let Some(mut node) = world.get_mut::<Node>(target) {
                            node.flex_grow = *weight;
                        }
                    }
                    if child_zoomed.is_some() {
                        zoomed = child_zoomed;
                    }
                    ordered.push(target);
                }
                self.children(world, entity, &ordered);
                (entity, zoomed)
            }
            LayoutNode::Pane {
                id,
                stacks,
                is_zoomed,
            } => {
                let entity = match id.as_deref() {
                    Some(id) => Self::parse(id).unwrap().1,
                    None => world.spawn((Pane::bundle(), LastActivatedAt(0))).id(),
                };
                world.entity_mut(entity).insert(ChildOf(parent));
                self.host_window(world, entity, parent);
                let mut ordered = Vec::new();
                for stack in stacks {
                    ordered.push(self.stack(world, stack, entity));
                }
                self.children(world, entity, &ordered);
                (entity, is_zoomed.then_some(entity))
            }
        }
    }

    fn split(
        &self,
        world: &mut World,
        target: Option<Entity>,
        parent: Entity,
        direction: PaneSplitDirection,
    ) -> Entity {
        let entity = match target {
            Some(entity) => entity,
            None => world
                .spawn((Pane::split_bundle(direction), LastActivatedAt(0)))
                .id(),
        };
        world
            .entity_mut(entity)
            .insert((PaneSplit { direction }, ChildOf(parent)));
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            node.flex_direction = direction.flex_direction();
            let gap = direction.gaps(crate::event::PANE_GAP_PX);
            node.column_gap = gap.column_gap;
            node.row_gap = gap.row_gap;
        }
        self.host_window(world, entity, parent);
        entity
    }

    fn host_window(&self, world: &mut World, entity: Entity, parent: Entity) {
        if world.get::<Tab>(parent).is_none() {
            world.entity_mut(entity).remove::<HostWindow>();
            return;
        }
        let mut ancestor = parent;
        loop {
            if let Some(host) = world.get::<HostWindow>(ancestor) {
                let window = host.0;
                world.entity_mut(entity).insert(HostWindow(window));
                return;
            }
            let Some(parent) = world.get::<ChildOf>(ancestor) else {
                return;
            };
            ancestor = parent.parent();
        }
    }

    fn stack(&self, world: &mut World, stack: &proto::Stack, parent: Entity) -> Entity {
        if let Some(id) = stack.id.as_deref() {
            let entity = Self::parse(id).unwrap().1;
            world.entity_mut(entity).insert(ChildOf(parent));
            if !stack.title.is_empty()
                && let Some(mut metadata) = world.get_mut::<PageMetadata>(entity)
            {
                metadata.title = stack.title.clone();
            }
            return entity;
        }
        let entity = world
            .spawn((
                Stack::bundle(),
                CreatedAt::now(),
                LastActivatedAt(0),
                ChildOf(parent),
            ))
            .id();
        if stack.kind == "terminal" {
            world.write_message(TerminalLayoutSpawnRequest { stack: entity });
        } else {
            world.write_message(PageOpenRequest {
                target: PageOpenTarget::Stack(entity),
                url: stack.url.clone(),
                request_id: None,
            });
        }
        entity
    }

    pub(super) fn children(&self, world: &mut World, parent: Entity, desired: &[Entity]) {
        let mut children = desired.to_vec();
        if let Some(current) = world.get::<Children>(parent) {
            for child in current.iter() {
                if world.get::<Tab>(child).is_none()
                    && world.get::<Pane>(child).is_none()
                    && world.get::<Stack>(child).is_none()
                {
                    children.push(child);
                }
            }
        }
        world.entity_mut(parent).replace_children(&children);
    }

    pub(super) fn zoom(&self, world: &mut World, tab: Entity, leaf: Option<Entity>) {
        world
            .entity_mut(tab)
            .remove::<crate::host::zoom::PaneZoomTransition>();
        if let Some(scheduler) = world.get_resource::<crate::host::deadline::DeadlineScheduler>() {
            scheduler.cancel(crate::host::deadline::DeadlineOwner::PaneZoomAnimation(tab));
        }
        world.entity_mut(tab).remove::<Zoomed>();
        let Some(leaf) = leaf else {
            return;
        };
        let mut hidden = Vec::new();
        let mut current = leaf;
        while let Some(parent) = world.get::<ChildOf>(current).map(ChildOf::parent) {
            if parent == tab {
                break;
            }
            if let Some(children) = world.get::<Children>(parent) {
                for sibling in children.iter() {
                    if sibling != current && world.get::<Pane>(sibling).is_some() {
                        hidden.push(sibling);
                    }
                }
            }
            current = parent;
        }
        world.entity_mut(tab).insert(Zoomed { leaf, hidden });
    }

    pub(super) fn focus(&self, world: &mut World) {
        let focus = &self.snapshot.focused;
        if let Some(id) = focus
            .stack
            .as_deref()
            .or(focus.pane.as_deref())
            .or(focus.tab.as_deref())
        {
            world.trigger(vmux_ecs::ActivateRequest {
                entity: Self::parse(id).unwrap().1,
            });
        }
    }
}
