//! Turning a reconciled layout into spawned panes and stacks.
//!
//! The peer of `reconcile`, which plans the change: that half is pure and compiles everywhere,
//! while this one needs Bevy and an ECS world the browser bundle has neither of, so it is gated
//! as a whole.

use std::collections::HashMap;
use std::collections::HashSet as ApplyHashSet;

use crate::protocol::{LayoutNode, LayoutSnapshot, NodeKind, parse_id};
use crate::reconcile::*;

use crate::pane::{
    Pane, PaneSize, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps,
    split_root_bundle,
};
use crate::protocol as proto;
use crate::protocol::format_id;
use crate::stack::{Stack, stack_bundle};
use crate::tab::Tab as LayoutTab;
use crate::{LayoutSpawnRequest, event::PANE_GAP_PX};
use bevy::ecs::message::{MessageReader, MessageWriter, Messages};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_history::{CreatedAt, LastActivatedAt};

#[derive(Message, Clone)]
pub struct LayoutApplyRequest {
    pub request_id: [u8; 16],
    pub snapshot: LayoutSnapshot,
}

#[derive(Message, Clone)]
pub struct LayoutApplyResponse {
    pub request_id: [u8; 16],
    pub result: Result<LayoutSnapshot, String>,
}

#[derive(Message, Clone)]
pub struct LayoutSnapshotRequest {
    pub request_id: [u8; 16],
    pub anchor: Option<vmux_core::ProcessId>,
}

#[derive(Message, Clone)]
pub struct LayoutSnapshotResponse {
    pub request_id: [u8; 16],
    pub snapshot: LayoutSnapshot,
}

pub fn serve_snapshot_requests(
    mut reader: MessageReader<LayoutSnapshotRequest>,
    tabs_q: Query<(Entity, &LayoutTab, Option<&Children>)>,
    splits_q: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: Query<(Entity, Option<&Children>, Option<&vmux_core::PageMetadata>), With<Stack>>,
    pane_sizes_q: Query<&PaneSize>,
    zoomed_q: Query<&crate::pane::Zoomed>,
    focused: Res<crate::stack::FocusedStack>,
    process_ids: Query<(&vmux_core::ProcessId, &ChildOf)>,
    child_of_q: Query<&ChildOf>,
    space_q: Query<(), With<crate::space::Space>>,
    active_space_q: Query<Entity, (With<crate::space::Space>, With<vmux_core::Active>)>,
    mut writer: MessageWriter<LayoutSnapshotResponse>,
) {
    let pid_by_stack: HashMap<u64, String> = process_ids
        .iter()
        .map(|(pid, co)| (co.get().to_bits(), pid.to_string()))
        .collect();
    for request in reader.read() {
        let self_stack = request.anchor.and_then(|anchor| {
            process_ids
                .iter()
                .find(|(pid, _)| **pid == anchor)
                .map(|(_, co)| co.get())
        });
        let target_space = self_stack
            .and_then(|stack| crate::space::space_of(stack, &child_of_q, &space_q))
            .or_else(|| active_space_q.iter().next());
        let mut snapshot = crate::snapshot::build_layout_snapshot(
            &tabs_q,
            &splits_q,
            &leaves_q,
            &stacks_q,
            &pane_sizes_q,
            &zoomed_q,
            &focused,
            self_stack,
        );
        if let Some(target) = target_space {
            snapshot.tabs.retain(|tab| {
                tab.id
                    .as_deref()
                    .and_then(|id| crate::protocol::parse_id(id).ok())
                    .map(|(_, bits)| {
                        crate::space::space_of(Entity::from_bits(bits), &child_of_q, &space_q)
                            == Some(target)
                    })
                    .unwrap_or(true)
            });
        }
        for tab in &mut snapshot.tabs {
            fill_process_ids(&mut tab.root, &pid_by_stack);
        }
        writer.write(LayoutSnapshotResponse {
            request_id: request.request_id,
            snapshot,
        });
    }
}

fn fill_process_ids(node: &mut LayoutNode, pid_by_stack: &HashMap<u64, String>) {
    match node {
        LayoutNode::Split { children, .. } => {
            for child in children {
                fill_process_ids(child, pid_by_stack);
            }
        }
        LayoutNode::Pane { stacks, .. } => {
            for stack in stacks {
                if let Some(id) = &stack.id
                    && let Ok((NodeKind::Stack, bits)) = parse_id(id)
                    && let Some(pid) = pid_by_stack.get(&bits)
                {
                    stack.process_id = Some(pid.clone());
                }
            }
        }
    }
}

pub fn apply_layout_requests(
    mut reader: MessageReader<LayoutApplyRequest>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let snapshot = request.snapshot.clone();
        let request_id = request.request_id;
        commands.queue(move |world: &mut World| {
            let result = match apply(world, &snapshot) {
                Ok(()) => {
                    let snapshot = run_build_snapshot(world);
                    Ok(snapshot)
                }
                Err(err) => Err(format!("update_layout: {err:?}")),
            };
            world
                .resource_mut::<Messages<LayoutApplyResponse>>()
                .write(LayoutApplyResponse { request_id, result });
        });
    }
}

fn run_build_snapshot(world: &mut World) -> LayoutSnapshot {
    use bevy::ecs::system::SystemState;
    let mut state = SystemState::<(
        Query<(Entity, &LayoutTab, Option<&Children>)>,
        Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
        Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
        Query<(Entity, Option<&Children>, Option<&vmux_core::PageMetadata>), With<Stack>>,
        Query<&PaneSize>,
        Query<&crate::pane::Zoomed>,
        Res<crate::stack::FocusedStack>,
    )>::new(world);
    let (tabs, splits, leaves, stacks, pane_sizes, zoomed, focused) = state.get(world).unwrap();
    crate::snapshot::build_layout_snapshot(
        &tabs,
        &splits,
        &leaves,
        &stacks,
        &pane_sizes,
        &zoomed,
        &focused,
        None,
    )
}

pub fn apply(world: &mut World, snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    let existing = collect_existing_ids(world);
    apply_with_existing(world, snapshot, &existing)
}

pub fn apply_with_existing(
    world: &mut World,
    snapshot: &LayoutSnapshot,
    existing: &ApplyHashSet<String>,
) -> Result<(), ValidationError> {
    let plan = plan_diff(snapshot, existing)?;

    let mut new_entities: std::collections::HashMap<*const proto::LayoutNode, Entity> =
        std::collections::HashMap::new();
    // Resolve (or create) each tab's entity once and keep the pairing, so the
    // structure pass reparents existing nodes into NEW tabs too (e.g. moving a
    // stack to a brand-new tab) — not only into tabs that already have an id.
    let mut materialized: Vec<(&proto::Tab, Entity)> = Vec::with_capacity(snapshot.tabs.len());
    // The container existing tabs hang off, so new tabs can be spawned as their
    // siblings (the tab strip only shows same-parent, same-space siblings).
    let tab_parent: Option<Entity> = snapshot
        .tabs
        .iter()
        .filter_map(|t| t.id.as_deref())
        .filter_map(|id| parse_id(id).ok())
        .map(|(_, v)| Entity::from_bits(v))
        .find_map(|e| world.get::<ChildOf>(e).map(|c| c.parent()));
    for tab in &snapshot.tabs {
        let tab_entity = match &tab.id {
            Some(id) => match parse_id(id) {
                Ok((_, value)) => Entity::from_bits(value),
                Err(_) => continue,
            },
            None => {
                let entity = world
                    .spawn((
                        crate::tab::tab_bundle(),
                        LastActivatedAt::now(),
                        CreatedAt::now(),
                    ))
                    .id();
                // Match canonical tab creation: parent the new tab to the same
                // container as existing tabs and tag it with the active space.
                // Otherwise the sibling-grouped, space-scoped tab strip filters it
                // out and it never shows, even though it exists in the tree.
                if let Some(parent) = tab_parent {
                    world.entity_mut(entity).insert(ChildOf(parent));
                }
                if !tab.name.is_empty()
                    && let Some(mut layout_tab) = world.get_mut::<LayoutTab>(entity)
                {
                    layout_tab.name = tab.name.clone();
                }
                entity
            }
        };
        materialize_descendants(world, tab_entity, &tab.root, &mut new_entities);
        materialized.push((tab, tab_entity));
    }

    for (tab, tab_entity) in &materialized {
        apply_structure(world, Some(*tab_entity), &tab.root, &new_entities);
    }
    for tab in &snapshot.tabs {
        apply_tab(world, tab);
    }
    // Honor the snapshot's active tab: make it the most-recently-activated tab so
    // the timestamp-based active-tab selection (windowed-browser visibility, focus
    // ring) follows `is_active` instead of defaulting to whichever tab was just
    // spawned — otherwise a newly created tab steals "active" and its windowed
    // browser covers the layout.
    if let Some((_, active_entity)) = materialized.iter().find(|(t, _)| t.is_active) {
        let newest = materialized
            .iter()
            .filter_map(|(_, e)| world.get::<LastActivatedAt>(*e).map(|l| l.0))
            .max()
            .unwrap_or(0);
        if let Ok(mut e) = world.get_entity_mut(*active_entity) {
            e.insert(LastActivatedAt(newest + 1));
        }
    }
    let rescued: ApplyHashSet<String> = new_entities
        .iter()
        .filter_map(|(ptr, &entity)| {
            let node = unsafe { &**ptr };
            let kind = match node {
                proto::LayoutNode::Split { .. } => NodeKind::Split,
                proto::LayoutNode::Pane { .. } => NodeKind::Pane,
            };
            let id = format_id(kind, entity.to_bits());
            existing.contains(&id).then_some(id)
        })
        .collect();
    for id in &plan.closes {
        if rescued.contains(id) {
            continue;
        }
        apply_close(world, id);
    }
    apply_focus(world, &snapshot.focused);
    Ok(())
}

fn materialize_descendants(
    world: &mut World,
    parent: Entity,
    node: &proto::LayoutNode,
    new_entities: &mut std::collections::HashMap<*const proto::LayoutNode, Entity>,
) {
    let node_entity = match node {
        proto::LayoutNode::Split { id, direction, .. } => match id {
            Some(id_str) => match parse_id(id_str) {
                Ok((_, v)) => Entity::from_bits(v),
                Err(_) => return,
            },
            None => {
                if world.get::<LayoutTab>(parent).is_some()
                    && let Some(existing_root) = find_root_split_child(world, parent)
                {
                    set_split_direction(world, existing_root, *direction);
                    new_entities.insert(node as *const _, existing_root);
                    existing_root
                } else {
                    let pane_split_dir = match direction {
                        proto::SplitDirection::Row => PaneSplitDirection::Row,
                        proto::SplitDirection::Column => PaneSplitDirection::Column,
                    };
                    let entity = world
                        .spawn((
                            split_root_bundle(pane_split_dir),
                            LastActivatedAt::now(),
                            ChildOf(parent),
                        ))
                        .id();
                    new_entities.insert(node as *const _, entity);
                    entity
                }
            }
        },
        proto::LayoutNode::Pane { id, .. } => match id {
            Some(id_str) => match parse_id(id_str) {
                Ok((_, v)) => Entity::from_bits(v),
                Err(_) => return,
            },
            None => {
                let entity = world
                    .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(parent)))
                    .id();
                new_entities.insert(node as *const _, entity);
                entity
            }
        },
    };

    match node {
        proto::LayoutNode::Split { children, .. } => {
            for c in children {
                materialize_descendants(world, node_entity, c, new_entities);
            }
        }
        proto::LayoutNode::Pane { stacks, .. } => {
            for t in stacks {
                if t.id.is_none() {
                    let stack = world
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(node_entity)))
                        .id();
                    match t.kind.as_str() {
                        "terminal" => {
                            world
                                .resource_mut::<Messages<LayoutSpawnRequest>>()
                                .write(LayoutSpawnRequest::Terminal { stack });
                        }
                        _ => {
                            world.resource_mut::<Messages<PageOpenRequest>>().write(
                                PageOpenRequest {
                                    target: PageOpenTarget::Stack(stack),
                                    url: t.url.clone(),
                                    request_id: None,
                                },
                            );
                        }
                    }
                }
            }
        }
    }
}

fn find_root_split_child(world: &World, tab: Entity) -> Option<Entity> {
    world
        .get::<Children>(tab)?
        .iter()
        .find(|&e| world.get::<PaneSplit>(e).is_some())
}

fn set_split_direction(world: &mut World, entity: Entity, direction: proto::SplitDirection) {
    let pane_split_dir = match direction {
        proto::SplitDirection::Row => PaneSplitDirection::Row,
        proto::SplitDirection::Column => PaneSplitDirection::Column,
    };
    if let Some(mut split) = world.get_mut::<PaneSplit>(entity) {
        split.direction = pane_split_dir;
    }
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.flex_direction = match pane_split_dir {
            PaneSplitDirection::Row => bevy::ui::FlexDirection::Row,
            PaneSplitDirection::Column => bevy::ui::FlexDirection::Column,
        };
        let gap = pane_split_gaps(pane_split_dir, PANE_GAP_PX);
        node.column_gap = gap.column_gap;
        node.row_gap = gap.row_gap;
    }
}

fn apply_close(world: &mut World, id: &str) {
    let Ok((_kind, value)) = parse_id(id) else {
        return;
    };
    let entity = Entity::from_bits(value);
    if let Ok(entity_ref) = world.get_entity_mut(entity) {
        entity_ref.despawn();
    }
}

fn collect_ids_recursive(world: &World, entity: Entity, out: &mut ApplyHashSet<String>) {
    let Ok(entity_ref) = world.get_entity(entity) else {
        return;
    };
    if entity_ref.contains::<LayoutTab>() {
        out.insert(format_id(NodeKind::Tab, entity.to_bits()));
    } else if entity_ref.contains::<PaneSplit>() {
        out.insert(format_id(NodeKind::Split, entity.to_bits()));
    } else if entity_ref.contains::<Pane>() {
        out.insert(format_id(NodeKind::Pane, entity.to_bits()));
    } else if entity_ref.contains::<Stack>() {
        out.insert(format_id(NodeKind::Stack, entity.to_bits()));
    }
    if let Some(children) = entity_ref.get::<Children>() {
        let kids: Vec<Entity> = children.iter().collect();
        for child in kids {
            collect_ids_recursive(world, child, out);
        }
    }
}

/// Existing ids the reconcile diff may add/remove. Scoped to the active space's
/// tab subtrees so `update_layout` can never despawn another space's content.
/// When there is no active space, all tabs are included (global behavior).
fn collect_existing_ids(world: &mut World) -> ApplyHashSet<String> {
    let mut active_space_q =
        world.query_filtered::<Entity, (With<crate::space::Space>, With<vmux_core::Active>)>();
    let active_space = active_space_q.iter(world).next();
    let mut tab_q = world.query_filtered::<(Entity, Option<&ChildOf>), With<LayoutTab>>();
    let tabs: Vec<Entity> = tab_q
        .iter(world)
        .filter(|(_, child_of)| {
            active_space.is_none() || child_of.map(|c| c.parent()) == active_space
        })
        .map(|(entity, _)| entity)
        .collect();
    let mut out = ApplyHashSet::new();
    for tab in tabs {
        collect_ids_recursive(world, tab, &mut out);
    }
    out
}

fn apply_tab(world: &mut World, tab: &proto::Tab) {
    if let Some(id) = &tab.id
        && let Ok((_, value)) = parse_id(id)
    {
        let entity = Entity::from_bits(value);
        if let Some(mut layout_tab) = world.get_mut::<LayoutTab>(entity) {
            layout_tab.name = tab.name.clone();
        }
    }
    apply_node(world, &tab.root);
}

fn apply_structure(
    world: &mut World,
    parent: Option<Entity>,
    node: &proto::LayoutNode,
    new_entities: &std::collections::HashMap<*const proto::LayoutNode, Entity>,
) {
    let Some(entity) = resolve_node_entity(node, new_entities) else {
        match node {
            proto::LayoutNode::Split { children, .. } => {
                for c in children {
                    apply_structure(world, parent, c, new_entities);
                }
            }
            proto::LayoutNode::Pane { .. } => {}
        }
        return;
    };
    if let Some(parent) = parent
        && let Ok(mut e) = world.get_entity_mut(entity)
    {
        e.insert(ChildOf(parent));
    }
    match node {
        proto::LayoutNode::Split { children, .. } => {
            for c in children {
                apply_structure(world, Some(entity), c, new_entities);
            }
        }
        proto::LayoutNode::Pane { stacks, .. } => {
            for t in stacks {
                if let Some(tid) = t.id.as_deref()
                    && let Ok((_, value)) = parse_id(tid)
                {
                    let tab_entity = Entity::from_bits(value);
                    if let Ok(mut e) = world.get_entity_mut(tab_entity) {
                        e.insert(ChildOf(entity));
                    }
                }
            }
        }
    }
}

fn resolve_node_entity(
    node: &proto::LayoutNode,
    new_entities: &std::collections::HashMap<*const proto::LayoutNode, Entity>,
) -> Option<Entity> {
    let id = match node {
        proto::LayoutNode::Split { id, .. } | proto::LayoutNode::Pane { id, .. } => id.as_deref(),
    };
    if let Some(id_str) = id {
        parse_id(id_str).ok().map(|(_, v)| Entity::from_bits(v))
    } else {
        new_entities.get(&(node as *const _)).copied()
    }
}

fn apply_node(world: &mut World, node: &proto::LayoutNode) {
    match node {
        proto::LayoutNode::Split {
            id,
            direction,
            flex_weights,
            children,
        } => {
            if let Some(id) = id
                && let Ok((_, value)) = parse_id(id)
            {
                let entity = Entity::from_bits(value);
                let pane_split_dir = match direction {
                    proto::SplitDirection::Row => PaneSplitDirection::Row,
                    proto::SplitDirection::Column => PaneSplitDirection::Column,
                };
                if let Some(mut split) = world.get_mut::<PaneSplit>(entity) {
                    split.direction = pane_split_dir;
                }
                if let Some(mut node) = world.get_mut::<Node>(entity) {
                    node.flex_direction = match pane_split_dir {
                        PaneSplitDirection::Row => bevy::ui::FlexDirection::Row,
                        PaneSplitDirection::Column => bevy::ui::FlexDirection::Column,
                    };
                    let gap = pane_split_gaps(pane_split_dir, PANE_GAP_PX);
                    node.column_gap = gap.column_gap;
                    node.row_gap = gap.row_gap;
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
        proto::LayoutNode::Pane { stacks, .. } => {
            for t in stacks {
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

fn apply_focus(world: &mut World, focus: &proto::Focus) {
    let Some(mut focused) = world.get_resource_mut::<crate::stack::FocusedStack>() else {
        return;
    };
    if let Some(id) = focus.tab.as_deref() {
        focused.tab = parse_id(id).ok().map(|(_, v)| Entity::from_bits(v));
    }
    if let Some(id) = focus.pane.as_deref() {
        focused.pane = parse_id(id).ok().map(|(_, v)| Entity::from_bits(v));
    }
    if let Some(id) = focus.stack.as_deref() {
        focused.stack = parse_id(id).ok().map(|(_, v)| Entity::from_bits(v));
    }
}

fn node_entity(node: &proto::LayoutNode) -> Option<Entity> {
    match node {
        proto::LayoutNode::Split { id, .. } | proto::LayoutNode::Pane { id, .. } => id
            .as_deref()
            .and_then(|id| parse_id(id).ok().map(|(_, value)| Entity::from_bits(value))),
    }
}

#[cfg(test)]
#[path = "apply.test.rs"]
mod tests;
