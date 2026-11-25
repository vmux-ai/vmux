use bevy::ecs::system::{Commands, SystemParam};
use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use vmux_ecs::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_flex::prelude::*;
use vmux_history::{CreatedAt, LastActivatedAt};

use crate::TerminalLayoutSpawnRequest;
use crate::host::zoom::{PaneZoomAnimation, PaneZoomTransition};
use crate::pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, Zoomed};
use crate::protocol::{self as proto, LayoutNode};
use crate::stack::Stack;
use crate::tab::Tab;
use crate::window::WindowHierarchy;
use vmux_ecs::time::Deadline;

use super::Reconcile;

#[derive(SystemParam)]
#[allow(unknown_lints, vmux_ecs_ownership)]
pub(super) struct Topology<'w, 's> {
    pub(super) commands: Commands<'w, 's>,
    tabs: Query<'w, 's, &'static Tab>,
    children: Query<'w, 's, &'static Children>,
    nodes: Query<'w, 's, (), Or<(With<Tab>, With<Pane>, With<Stack>)>>,
    windows: WindowHierarchy<'w, 's>,
    page_open: MessageWriter<'w, PageOpenRequest>,
    terminal: MessageWriter<'w, TerminalLayoutSpawnRequest>,
}

impl Topology<'_, '_> {
    pub(super) fn window(&self, scope: Option<Entity>) -> Option<Entity> {
        scope.and_then(|scope| self.windows.get(scope))
    }

    pub(super) fn tab(&mut self, tab: &proto::Tab, scope: Option<Entity>) -> Entity {
        let entity = match tab.id.as_deref() {
            Some(id) => Reconcile::parse(id).unwrap().1,
            None => self
                .commands
                .spawn((Tab::bundle(), CreatedAt::now(), LastActivatedAt(0)))
                .id(),
        };
        let startup_dir = self
            .tabs
            .get(entity)
            .ok()
            .and_then(|tab| tab.startup_dir.clone());
        self.commands.entity(entity).insert(Tab {
            name: tab.name.clone(),
            startup_dir,
        });
        if let Some(scope) = scope {
            self.commands.entity(entity).insert(ChildOf(scope));
        }
        entity
    }

    pub(super) fn node(
        &mut self,
        node: &LayoutNode,
        parent: Entity,
        reuse: Option<Entity>,
        root: bool,
        window: Option<Entity>,
    ) -> (Entity, Option<Entity>, Vec<Entity>) {
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
                let target = id
                    .as_deref()
                    .map(|id| Reconcile::parse(id).unwrap().1)
                    .or(reuse);
                let entity = self.split(target, parent, direction);
                self.host_window(entity, root, window);
                let mut ordered = Vec::new();
                let mut zoomed = None;
                let mut branch = None;
                let mut hidden = Vec::new();
                for (index, child) in children.iter().enumerate() {
                    let (target, child_zoomed, child_hidden) =
                        self.node(child, entity, None, false, window);
                    if let Some(weight) = flex_weights.get(index) {
                        self.weight(target, *weight);
                    }
                    if child_zoomed.is_some() {
                        zoomed = child_zoomed;
                        branch = Some(target);
                        hidden.extend(child_hidden);
                    }
                    ordered.push(target);
                }
                if let Some(branch) = branch {
                    for &sibling in &ordered {
                        if sibling != branch {
                            hidden.push(sibling);
                        }
                    }
                }
                self.children(entity, &ordered);
                (entity, zoomed, hidden)
            }
            LayoutNode::Pane {
                id,
                stacks,
                is_zoomed,
            } => {
                let entity = match id.as_deref() {
                    Some(id) => Reconcile::parse(id).unwrap().1,
                    None => self
                        .commands
                        .spawn((Pane::bundle(), LastActivatedAt(0)))
                        .id(),
                };
                self.commands.entity(entity).insert(ChildOf(parent));
                self.host_window(entity, root, window);
                let mut ordered = Vec::new();
                for stack in stacks {
                    ordered.push(self.stack(stack, entity));
                }
                self.children(entity, &ordered);
                (entity, is_zoomed.then_some(entity), Vec::new())
            }
        }
    }

    fn split(
        &mut self,
        target: Option<Entity>,
        parent: Entity,
        direction: PaneSplitDirection,
    ) -> Entity {
        let entity = match target {
            Some(entity) => entity,
            None => self
                .commands
                .spawn((Pane::split_bundle(direction), LastActivatedAt(0)))
                .id(),
        };
        self.commands
            .entity(entity)
            .insert((PaneSplit { direction }, ChildOf(parent)))
            .entry::<Node>()
            .and_modify(move |mut node| {
                node.flex_direction = direction.flex_direction();
                let gap = direction.gaps(crate::event::PANE_GAP_PX);
                node.column_gap = gap.column_gap;
                node.row_gap = gap.row_gap;
            });
        entity
    }

    fn weight(&mut self, entity: Entity, weight: f32) {
        self.commands
            .entity(entity)
            .insert(PaneSize { flex_grow: weight })
            .entry::<Node>()
            .and_modify(move |mut node| {
                node.flex_grow = weight;
            });
    }

    fn host_window(&mut self, entity: Entity, root: bool, window: Option<Entity>) {
        if !root {
            self.commands.entity(entity).remove::<HostWindow>();
            return;
        }
        if let Some(window) = window {
            self.commands.entity(entity).insert(HostWindow(window));
        }
    }

    fn stack(&mut self, stack: &proto::Stack, parent: Entity) -> Entity {
        if let Some(id) = stack.id.as_deref() {
            let entity = Reconcile::parse(id).unwrap().1;
            self.commands.entity(entity).insert(ChildOf(parent));
            if !stack.title.is_empty() {
                let title = stack.title.clone();
                self.commands
                    .entity(entity)
                    .entry::<PageMetadata>()
                    .and_modify(move |mut metadata| {
                        metadata.title = title;
                    });
            }
            return entity;
        }
        let entity = self
            .commands
            .spawn((
                Stack::bundle(),
                CreatedAt::now(),
                LastActivatedAt(0),
                ChildOf(parent),
            ))
            .id();
        if stack.kind == "terminal" {
            self.terminal
                .write(TerminalLayoutSpawnRequest { stack: entity });
        } else {
            self.page_open.write(PageOpenRequest {
                target: PageOpenTarget::Stack(entity),
                url: stack.url.clone(),
                request_id: None,
            });
        }
        entity
    }

    pub(super) fn children(&mut self, parent: Entity, desired: &[Entity]) {
        let mut children = desired.to_vec();
        if let Ok(current) = self.children.get(parent) {
            for child in current.iter() {
                if !self.nodes.contains(child) {
                    children.push(child);
                }
            }
        }
        self.commands.entity(parent).replace_children(&children);
    }

    pub(super) fn zoom(&mut self, tab: Entity, leaf: Option<Entity>, hidden: Vec<Entity>) {
        self.commands
            .entity(tab)
            .remove::<(PaneZoomTransition, Deadline<PaneZoomAnimation>)>();
        self.commands.entity(tab).remove::<Zoomed>();
        if let Some(leaf) = leaf {
            self.commands.entity(tab).insert(Zoomed { leaf, hidden });
        }
    }

    pub(super) fn focus(&mut self, focus: &proto::Focus) {
        if let Some(id) = focus
            .stack
            .as_deref()
            .or(focus.pane.as_deref())
            .or(focus.tab.as_deref())
        {
            self.commands.trigger(vmux_ecs::ActivateRequest {
                entity: Reconcile::parse(id).unwrap().1,
            });
        }
    }
}
