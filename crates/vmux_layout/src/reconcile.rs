#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use crate::protocol::{Focus, LayoutNode, LayoutSnapshot, NodeKind, Stack as StackDto, parse_id};

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    DuplicateId(String),
    InvalidIdFormat(String),
    WrongKindForPosition {
        id: String,
        expected: NodeKind,
        got: NodeKind,
    },
    NewStackMissingUrl,
    NewStackMissingKind,
    NewPaneMissingStacks,
    NewTabMissingName,
    FlexWeightsLengthMismatch {
        children: usize,
        weights: usize,
    },
    FocusReferencesUnknownId(String),
    MissingReferencedEntity(Vec<String>),
}

pub fn validate(snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut all_ids: HashSet<String> = HashSet::new();

    for tab in &snapshot.tabs {
        if let Some(id) = &tab.id {
            let (kind, _) =
                parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
            if kind != NodeKind::Tab {
                return Err(ValidationError::WrongKindForPosition {
                    id: id.clone(),
                    expected: NodeKind::Tab,
                    got: kind,
                });
            }
            if !seen.insert(id.clone()) {
                return Err(ValidationError::DuplicateId(id.clone()));
            }
            all_ids.insert(id.clone());
        } else if tab.name.is_empty() {
            return Err(ValidationError::NewTabMissingName);
        }
        validate_node(&tab.root, &mut seen, &mut all_ids)?;
    }

    validate_focus(&snapshot.focused, &all_ids)?;
    Ok(())
}

fn validate_node(
    node: &LayoutNode,
    seen: &mut HashSet<String>,
    all_ids: &mut HashSet<String>,
) -> Result<(), ValidationError> {
    match node {
        LayoutNode::Split {
            id,
            flex_weights,
            children,
            ..
        } => {
            if let Some(id) = id {
                let (kind, _) =
                    parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
                if kind != NodeKind::Split {
                    return Err(ValidationError::WrongKindForPosition {
                        id: id.clone(),
                        expected: NodeKind::Split,
                        got: kind,
                    });
                }
                if !seen.insert(id.clone()) {
                    return Err(ValidationError::DuplicateId(id.clone()));
                }
                all_ids.insert(id.clone());
            }
            if !flex_weights.is_empty() && flex_weights.len() != children.len() {
                return Err(ValidationError::FlexWeightsLengthMismatch {
                    children: children.len(),
                    weights: flex_weights.len(),
                });
            }
            for child in children {
                validate_node(child, seen, all_ids)?;
            }
            Ok(())
        }
        LayoutNode::Pane { id, stacks, .. } => {
            if let Some(id) = id {
                let (kind, _) =
                    parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
                if kind != NodeKind::Pane {
                    return Err(ValidationError::WrongKindForPosition {
                        id: id.clone(),
                        expected: NodeKind::Pane,
                        got: kind,
                    });
                }
                if !seen.insert(id.clone()) {
                    return Err(ValidationError::DuplicateId(id.clone()));
                }
                all_ids.insert(id.clone());
            } else if stacks.is_empty() {
                return Err(ValidationError::NewPaneMissingStacks);
            }
            for stack in stacks {
                validate_stack(stack, seen, all_ids)?;
            }
            Ok(())
        }
    }
}

fn validate_stack(
    stack: &StackDto,
    seen: &mut HashSet<String>,
    all_ids: &mut HashSet<String>,
) -> Result<(), ValidationError> {
    if let Some(id) = &stack.id {
        let (kind, _) = parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
        if kind != NodeKind::Stack {
            return Err(ValidationError::WrongKindForPosition {
                id: id.clone(),
                expected: NodeKind::Stack,
                got: kind,
            });
        }
        if !seen.insert(id.clone()) {
            return Err(ValidationError::DuplicateId(id.clone()));
        }
        all_ids.insert(id.clone());
    } else {
        if stack.url.is_empty() {
            return Err(ValidationError::NewStackMissingUrl);
        }
        if stack.kind.is_empty() {
            return Err(ValidationError::NewStackMissingKind);
        }
    }
    Ok(())
}

fn validate_focus(focus: &Focus, all_ids: &HashSet<String>) -> Result<(), ValidationError> {
    for id in [&focus.tab, &focus.pane, &focus.stack]
        .into_iter()
        .flatten()
    {
        if !all_ids.contains(id) {
            return Err(ValidationError::FocusReferencesUnknownId(id.clone()));
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum NodeAction {
    Match {
        existing: u64,
        desired_kind: NodeKind,
    },
    Create,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DiffPlan {
    pub actions_by_id: HashMap<String, NodeAction>,
    pub closes: Vec<String>,
    pub focus: Focus,
}

pub fn plan_diff(
    snapshot: &LayoutSnapshot,
    existing_ids: &HashSet<String>,
) -> Result<DiffPlan, ValidationError> {
    validate(snapshot)?;
    let mut actions_by_id: HashMap<String, NodeAction> = HashMap::new();
    let mut referenced: HashSet<String> = HashSet::new();

    for tab in &snapshot.tabs {
        if let Some(id) = &tab.id {
            referenced.insert(id.clone());
            let (_, value) = parse_id(id).expect("validated above");
            actions_by_id.insert(
                id.clone(),
                NodeAction::Match {
                    existing: value,
                    desired_kind: NodeKind::Tab,
                },
            );
        }
        plan_node(&tab.root, &mut actions_by_id, &mut referenced);
    }

    let mut missing: Vec<String> = referenced.difference(existing_ids).cloned().collect();
    if !missing.is_empty() {
        missing.sort();
        return Err(ValidationError::MissingReferencedEntity(missing));
    }

    let closes: Vec<String> = existing_ids.difference(&referenced).cloned().collect();

    Ok(DiffPlan {
        actions_by_id,
        closes,
        focus: snapshot.focused.clone(),
    })
}

fn plan_node(
    node: &LayoutNode,
    actions_by_id: &mut HashMap<String, NodeAction>,
    referenced: &mut HashSet<String>,
) {
    match node {
        LayoutNode::Split { id, children, .. } => {
            if let Some(id) = id {
                referenced.insert(id.clone());
                let (_, value) = parse_id(id).expect("validated");
                actions_by_id.insert(
                    id.clone(),
                    NodeAction::Match {
                        existing: value,
                        desired_kind: NodeKind::Split,
                    },
                );
            }
            for c in children {
                plan_node(c, actions_by_id, referenced);
            }
        }
        LayoutNode::Pane { id, stacks, .. } => {
            if let Some(id) = id {
                referenced.insert(id.clone());
                let (_, value) = parse_id(id).expect("validated");
                actions_by_id.insert(
                    id.clone(),
                    NodeAction::Match {
                        existing: value,
                        desired_kind: NodeKind::Pane,
                    },
                );
            }
            for t in stacks {
                if let Some(tid) = &t.id {
                    referenced.insert(tid.clone());
                    let (_, value) = parse_id(tid).expect("validated");
                    actions_by_id.insert(
                        tid.clone(),
                        NodeAction::Match {
                            existing: value,
                            desired_kind: NodeKind::Stack,
                        },
                    );
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashSet as ApplyHashSet;

#[cfg(not(target_arch = "wasm32"))]
use crate::pane::{
    Pane, PaneSize, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps,
    split_root_bundle,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::protocol as proto;
#[cfg(not(target_arch = "wasm32"))]
use crate::protocol::format_id;
#[cfg(not(target_arch = "wasm32"))]
use crate::stack::{Stack, stack_bundle};
#[cfg(not(target_arch = "wasm32"))]
use crate::tab::Tab as LayoutTab;
#[cfg(not(target_arch = "wasm32"))]
use crate::{LayoutSpawnRequest, event::PANE_GAP_PX};
#[cfg(not(target_arch = "wasm32"))]
use bevy::ecs::message::{MessageReader, MessageWriter, Messages};
#[cfg(not(target_arch = "wasm32"))]
use bevy::ecs::relationship::Relationship;
#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
#[cfg(not(target_arch = "wasm32"))]
use vmux_history::LastActivatedAt;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct LayoutApplyRequest {
    pub request_id: [u8; 16],
    pub snapshot: LayoutSnapshot,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct LayoutApplyResponse {
    pub request_id: [u8; 16],
    pub result: Result<LayoutSnapshot, String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct LayoutSnapshotRequest {
    pub request_id: [u8; 16],
    pub anchor: Option<vmux_core::ProcessId>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct LayoutSnapshotResponse {
    pub request_id: [u8; 16],
    pub snapshot: LayoutSnapshot,
}

#[cfg(not(target_arch = "wasm32"))]
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
    tab_space_q: Query<&crate::space::SpaceId>,
    active_id: Option<Res<crate::space::ActiveSpaceId>>,
    mut writer: MessageWriter<LayoutSnapshotResponse>,
) {
    let pid_by_stack: HashMap<u64, String> = process_ids
        .iter()
        .map(|(pid, co)| (co.get().to_bits(), pid.to_string()))
        .collect();
    let active_space = active_id.as_deref().and_then(|id| id.0.clone());
    for request in reader.read() {
        let self_stack = request.anchor.and_then(|anchor| {
            process_ids
                .iter()
                .find(|(pid, _)| **pid == anchor)
                .map(|(_, co)| co.get())
        });
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
        if let Some(active) = active_space.as_deref() {
            snapshot.tabs.retain(|tab| {
                tab.id
                    .as_deref()
                    .and_then(|id| crate::protocol::parse_id(id).ok())
                    .map(|(_, bits)| {
                        tab_space_q
                            .get(Entity::from_bits(bits))
                            .map(|sid| sid.0 == active)
                            .unwrap_or(true)
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn apply(world: &mut World, snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    let existing = collect_existing_ids(world);
    apply_with_existing(world, snapshot, &existing)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn apply_with_existing(
    world: &mut World,
    snapshot: &LayoutSnapshot,
    existing: &ApplyHashSet<String>,
) -> Result<(), ValidationError> {
    let plan = plan_diff(snapshot, existing)?;

    let mut new_entities: std::collections::HashMap<*const proto::LayoutNode, Entity> =
        std::collections::HashMap::new();
    for tab in &snapshot.tabs {
        let tab_entity = match &tab.id {
            Some(id) => match parse_id(id) {
                Ok((_, value)) => Entity::from_bits(value),
                Err(_) => continue,
            },
            None => {
                let entity = world
                    .spawn((crate::tab::tab_bundle(), LastActivatedAt::now()))
                    .id();
                if !tab.name.is_empty()
                    && let Some(mut layout_tab) = world.get_mut::<LayoutTab>(entity)
                {
                    layout_tab.name = tab.name.clone();
                }
                entity
            }
        };
        materialize_descendants(world, tab_entity, &tab.root, &mut new_entities);
    }

    for tab in &snapshot.tabs {
        if let Some(id) = &tab.id
            && let Ok((_, value)) = parse_id(id)
        {
            let tab_entity = Entity::from_bits(value);
            apply_structure(world, Some(tab_entity), &tab.root, &new_entities);
        }
    }
    for tab in &snapshot.tabs {
        apply_tab(world, tab);
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn find_root_split_child(world: &World, tab: Entity) -> Option<Entity> {
    world
        .get::<Children>(tab)?
        .iter()
        .find(|&e| world.get::<PaneSplit>(e).is_some())
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn apply_close(world: &mut World, id: &str) {
    let Ok((_kind, value)) = parse_id(id) else {
        return;
    };
    let entity = Entity::from_bits(value);
    if let Ok(entity_ref) = world.get_entity_mut(entity) {
        entity_ref.despawn();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn active_space_id(world: &World) -> Option<String> {
    world
        .get_resource::<crate::space::ActiveSpaceId>()
        .and_then(|active| active.0.clone())
}

#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
fn collect_existing_ids(world: &mut World) -> ApplyHashSet<String> {
    let active = active_space_id(world);
    let mut tab_q =
        world.query_filtered::<(Entity, Option<&crate::space::SpaceId>), With<LayoutTab>>();
    let tabs: Vec<Entity> = tab_q
        .iter(world)
        .filter(|(_, sid)| crate::space::in_active_space(*sid, active.as_deref()))
        .map(|(entity, _)| entity)
        .collect();
    let mut out = ApplyHashSet::new();
    for tab in tabs {
        collect_ids_recursive(world, tab, &mut out);
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn node_entity(node: &proto::LayoutNode) -> Option<Entity> {
    match node {
        proto::LayoutNode::Split { id, .. } | proto::LayoutNode::Pane { id, .. } => id
            .as_deref()
            .and_then(|id| parse_id(id).ok().map(|(_, value)| Entity::from_bits(value))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{SplitDirection, Tab as TabDto};

    #[test]
    fn collect_existing_ids_scoped_to_active_space() {
        let mut world = World::new();
        world.insert_resource(crate::space::ActiveSpaceId(Some("a".to_string())));
        let tab_a = world
            .spawn((
                crate::tab::Tab::default(),
                crate::space::SpaceId("a".to_string()),
            ))
            .id();
        let tab_b = world
            .spawn((
                crate::tab::Tab::default(),
                crate::space::SpaceId("b".to_string()),
            ))
            .id();
        let ids = collect_existing_ids(&mut world);
        assert!(ids.contains(&format_id(NodeKind::Tab, tab_a.to_bits())));
        assert!(!ids.contains(&format_id(NodeKind::Tab, tab_b.to_bits())));
    }

    fn pane(id: Option<&str>, stacks: Vec<StackDto>) -> LayoutNode {
        LayoutNode::Pane {
            id: id.map(str::to_string),
            is_zoomed: false,
            stacks,
        }
    }

    fn split(id: Option<&str>, children: Vec<LayoutNode>, weights: Vec<f32>) -> LayoutNode {
        LayoutNode::Split {
            id: id.map(str::to_string),
            direction: SplitDirection::Row,
            flex_weights: weights,
            children,
        }
    }

    fn snapshot(root: LayoutNode, focus: Focus) -> LayoutSnapshot {
        LayoutSnapshot {
            tabs: vec![TabDto {
                id: Some("tab:1".into()),
                name: "S".into(),
                is_active: true,
                root,
            }],
            focused: focus,
        }
    }

    #[test]
    fn validate_accepts_minimal_existing_layout() {
        let snap = snapshot(
            pane(
                Some("pane:2"),
                vec![StackDto {
                    id: Some("stack:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:2".into()),
                stack: Some("stack:3".into()),
            },
        );
        assert!(validate(&snap).is_ok());
    }

    #[test]
    fn validate_rejects_duplicate_pane_id() {
        let snap = snapshot(
            split(
                Some("split:1"),
                vec![pane(Some("pane:2"), vec![]), pane(Some("pane:2"), vec![])],
                vec![1.0, 1.0],
            ),
            Focus::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::DuplicateId(_))
        ));
    }

    #[test]
    fn validate_rejects_new_pane_without_tabs() {
        let snap = snapshot(pane(None, vec![]), Focus::default());
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::NewPaneMissingStacks)
        ));
    }

    #[test]
    fn validate_rejects_new_tab_without_url() {
        let snap = snapshot(
            pane(
                None,
                vec![StackDto {
                    id: None,
                    url: String::new(),
                    kind: "browser".into(),
                    ..Default::default()
                }],
            ),
            Focus::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::NewStackMissingUrl)
        ));
    }

    #[test]
    fn validate_rejects_new_tab_without_kind() {
        let snap = snapshot(
            pane(
                None,
                vec![StackDto {
                    id: None,
                    url: "https://x".into(),
                    kind: String::new(),
                    ..Default::default()
                }],
            ),
            Focus::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::NewStackMissingKind)
        ));
    }

    #[test]
    fn validate_rejects_focus_to_unknown_id() {
        let snap = snapshot(
            pane(
                Some("pane:2"),
                vec![StackDto {
                    id: Some("stack:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:99".into()),
                stack: None,
            },
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::FocusReferencesUnknownId(_))
        ));
    }

    #[test]
    fn validate_rejects_wrong_kind_in_position() {
        let snap = snapshot(pane(Some("stack:2"), vec![]), Focus::default());
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::WrongKindForPosition { .. })
        ));
    }

    #[test]
    fn validate_rejects_flex_weights_length_mismatch() {
        let snap = snapshot(
            split(
                Some("split:1"),
                vec![pane(
                    Some("pane:2"),
                    vec![StackDto {
                        id: Some("stack:3".into()),
                        ..Default::default()
                    }],
                )],
                vec![1.0, 2.0],
            ),
            Focus::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::FlexWeightsLengthMismatch { .. })
        ));
    }

    #[test]
    fn plan_marks_existing_ids_as_matches() {
        let snap = snapshot(
            pane(
                Some("pane:2"),
                vec![StackDto {
                    id: Some("stack:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:2".into()),
                stack: Some("stack:3".into()),
            },
        );
        let existing: HashSet<String> = ["tab:1", "pane:2", "stack:3"]
            .into_iter()
            .map(String::from)
            .collect();
        let plan = plan_diff(&snap, &existing).unwrap();
        assert!(plan.actions_by_id.contains_key("pane:2"));
        assert!(plan.actions_by_id.contains_key("stack:3"));
        assert!(plan.closes.is_empty());
    }

    #[test]
    fn plan_lists_unreferenced_ids_for_close() {
        let snap = snapshot(
            pane(
                Some("pane:2"),
                vec![StackDto {
                    id: Some("stack:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:2".into()),
                stack: Some("stack:3".into()),
            },
        );
        let existing: HashSet<String> = ["tab:1", "pane:2", "stack:3", "stack:4"]
            .into_iter()
            .map(String::from)
            .collect();
        let plan = plan_diff(&snap, &existing).unwrap();
        assert_eq!(plan.closes, vec!["stack:4".to_string()]);
    }

    #[test]
    fn plan_treats_id_omission_as_create() {
        let snap = snapshot(
            pane(
                None,
                vec![StackDto {
                    id: None,
                    url: "https://x".into(),
                    kind: "browser".into(),
                    ..Default::default()
                }],
            ),
            Focus {
                tab: Some("tab:1".into()),
                pane: None,
                stack: None,
            },
        );
        let existing: HashSet<String> = ["tab:1"].into_iter().map(String::from).collect();
        let plan = plan_diff(&snap, &existing).unwrap();
        assert!(plan.closes.is_empty());
        assert_eq!(plan.actions_by_id.len(), 1);
    }

    #[test]
    fn plan_rejects_referenced_tab_id_not_in_existing() {
        let snap = snapshot(
            pane(
                Some("pane:2"),
                vec![StackDto {
                    id: Some("stack:99".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:2".into()),
                stack: Some("stack:99".into()),
            },
        );
        let existing: HashSet<String> = ["tab:1", "pane:2"].into_iter().map(String::from).collect();
        match plan_diff(&snap, &existing) {
            Err(ValidationError::MissingReferencedEntity(ids)) => {
                assert!(
                    ids.contains(&"stack:99".to_string()),
                    "expected stale tab:99 in error, got {ids:?}"
                );
            }
            other => panic!("expected MissingReferencedEntity, got {other:?}"),
        }
    }

    #[test]
    fn plan_rejects_referenced_pane_id_not_in_existing() {
        let snap = snapshot(pane(Some("pane:42"), vec![]), Focus::default());
        let existing: HashSet<String> = ["tab:1"].into_iter().map(String::from).collect();
        assert!(matches!(
            plan_diff(&snap, &existing),
            Err(ValidationError::MissingReferencedEntity(_))
        ));
    }

    use crate::pane::{Pane, PaneSplitDirection};
    use crate::tab::Tab as LayoutTab;

    #[test]
    fn updating_split_direction_changes_component() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let split_e = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let _pane_a = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
        let _pane_b = app.world_mut().spawn((Pane, ChildOf(split_e))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(format_id(NodeKind::Split, split_e.to_bits())),
                    direction: proto::SplitDirection::Column,
                    flex_weights: vec![],
                    children: vec![],
                },
            }],
            focused: proto::Focus::default(),
        };

        apply(app.world_mut(), &snap).unwrap();
        let updated = app.world().get::<PaneSplit>(split_e).unwrap();
        assert_eq!(updated.direction, PaneSplitDirection::Column);
    }

    #[test]
    fn updating_flex_weights_writes_pane_size_flex_grow() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let split_e = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let pane_a = app
            .world_mut()
            .spawn((Pane, PaneSize { flex_grow: 1.0 }, ChildOf(split_e)))
            .id();
        let pane_b = app
            .world_mut()
            .spawn((Pane, PaneSize { flex_grow: 1.0 }, ChildOf(split_e)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(format_id(NodeKind::Split, split_e.to_bits())),
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![3.0, 1.0],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_a.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                        proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_b.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        apply(app.world_mut(), &snap).unwrap();
        assert_eq!(app.world().get::<PaneSize>(pane_a).unwrap().flex_grow, 3.0);
        assert_eq!(app.world().get::<PaneSize>(pane_b).unwrap().flex_grow, 1.0);
    }

    #[test]
    fn moves_pane_to_new_parent() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let split_a = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let split_b = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let moved = app.world_mut().spawn((Pane, ChildOf(split_a))).id();
        let _filler_b = app.world_mut().spawn((Pane, ChildOf(split_b))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(format_id(NodeKind::Split, split_a.to_bits())),
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![proto::LayoutNode::Split {
                        id: Some(format_id(NodeKind::Split, split_b.to_bits())),
                        direction: proto::SplitDirection::Row,
                        flex_weights: vec![],
                        children: vec![proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, moved.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        }],
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        apply(app.world_mut(), &snap).unwrap();
        let parent = app.world().get::<ChildOf>(moved).map(|p| p.parent());
        assert_eq!(parent, Some(split_b));
    }

    #[test]
    fn omitting_pane_from_snapshot_closes_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let split_e = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let keep = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
        let drop_me = app.world_mut().spawn((Pane, ChildOf(split_e))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(format_id(NodeKind::Split, split_e.to_bits())),
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, keep.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        let existing: std::collections::HashSet<String> = [
            format_id(NodeKind::Tab, tab.to_bits()),
            format_id(NodeKind::Split, split_e.to_bits()),
            format_id(NodeKind::Pane, keep.to_bits()),
            format_id(NodeKind::Pane, drop_me.to_bits()),
        ]
        .into_iter()
        .collect();

        apply_with_existing(app.world_mut(), &snap, &existing).unwrap();
        assert!(
            app.world().get_entity(drop_me).is_err(),
            "drop_me should be despawned"
        );
        assert!(app.world().get_entity(keep).is_ok(), "keep should survive");
    }

    #[test]
    fn apply_returns_error_for_stale_tab_id_does_not_panic() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let dead_stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane_e)))
            .id();
        app.world_mut().entity_mut(dead_stack).despawn();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: Some(format_id(NodeKind::Stack, dead_stack.to_bits())),
                        ..Default::default()
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        let result = apply(app.world_mut(), &snap);
        assert!(
            matches!(result, Err(ValidationError::MissingReferencedEntity(_))),
            "expected MissingReferencedEntity, got {result:?}"
        );
    }

    #[test]
    fn submitting_new_tab_id_none_spawns_stack_entity() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: None,
                        url: "https://example.com".into(),
                        kind: "browser".into(),
                        ..Default::default()
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        apply(app.world_mut(), &snap).unwrap();

        let stack_count = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(stack_count, 1, "one new Stack entity should be spawned");
    }

    #[test]
    fn malformed_pane_id_skips_subtree_no_orphan_spawn() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();

        let pane_count_before = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .count();

        let mut new_entities = std::collections::HashMap::new();
        let bad_node = proto::LayoutNode::Pane {
            id: Some("pane:not_a_number".into()),
            is_zoomed: false,
            stacks: vec![proto::Stack {
                id: None,
                url: "https://example.com".into(),
                kind: "browser".into(),
                ..Default::default()
            }],
        };
        materialize_descendants(app.world_mut(), tab, &bad_node, &mut new_entities);

        let pane_count_after = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            pane_count_before, pane_count_after,
            "malformed id must not spawn orphan pane"
        );

        let stack_count = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(stack_count, 0, "stacks under malformed pane must not spawn");
    }

    #[test]
    fn malformed_split_id_skips_subtree_no_orphan_spawn() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();

        let split_count_before = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .count();

        let mut new_entities = std::collections::HashMap::new();
        let bad_node = proto::LayoutNode::Split {
            id: Some("split:garbage".into()),
            direction: proto::SplitDirection::Row,
            flex_weights: vec![],
            children: vec![proto::LayoutNode::Pane {
                id: None,
                is_zoomed: false,
                stacks: vec![],
            }],
        };
        materialize_descendants(app.world_mut(), tab, &bad_node, &mut new_entities);

        let split_count_after = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            split_count_before, split_count_after,
            "malformed id must not spawn orphan split"
        );

        let pane_count = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            pane_count, 0,
            "children under malformed split must not spawn"
        );
    }

    #[test]
    fn reordering_split_children_swaps_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let split_e = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let pane_a = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
        let pane_b = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
        let pane_c = app.world_mut().spawn((Pane, ChildOf(split_e))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(format_id(NodeKind::Split, split_e.to_bits())),
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_c.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                        proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_a.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                        proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_b.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        apply(app.world_mut(), &snap).unwrap();

        let children = app
            .world()
            .get::<Children>(split_e)
            .expect("split has Children");
        let order: Vec<Entity> = children.iter().collect();
        assert_eq!(
            order,
            vec![pane_c, pane_a, pane_b],
            "Children should match submitted order"
        );
    }

    #[test]
    fn focus_change_writes_focused_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::stack::FocusedStack::default());

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane_e)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                        ..Default::default()
                    }],
                },
            }],
            focused: proto::Focus {
                tab: Some(format_id(NodeKind::Tab, tab.to_bits())),
                pane: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                stack: Some(format_id(NodeKind::Stack, stack.to_bits())),
            },
        };

        apply(app.world_mut(), &snap).unwrap();
        let focused = app.world().resource::<crate::stack::FocusedStack>();
        assert_eq!(focused.tab, Some(tab));
        assert_eq!(focused.pane, Some(pane_e));
        assert_eq!(focused.stack, Some(stack));
    }

    #[test]
    fn apply_focus_preserves_existing_when_dto_fields_omitted() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(crate::stack::FocusedStack::default());

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane_e)))
            .id();

        {
            let mut f = app.world_mut().resource_mut::<crate::stack::FocusedStack>();
            f.tab = Some(tab);
            f.pane = Some(pane_e);
            f.stack = Some(stack);
        }

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                        ..Default::default()
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        apply(app.world_mut(), &snap).unwrap();
        let f = app.world().resource::<crate::stack::FocusedStack>();
        assert_eq!(f.tab, Some(tab), "focused.tab must be preserved");
        assert_eq!(f.pane, Some(pane_e), "focused.pane must be preserved");
        assert_eq!(f.stack, Some(stack), "focused.stack must be preserved");
    }

    #[test]
    fn new_split_inserts_node_with_flex_direction() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();
        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: None,
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_e.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                        proto::LayoutNode::Pane {
                            id: None,
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: None,
                                url: "https://example.com".into(),
                                kind: "browser".into(),
                                ..Default::default()
                            }],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        apply(app.world_mut(), &snap).unwrap();

        let split_count = app
            .world_mut()
            .query_filtered::<&Node, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .filter(|node| node.flex_direction == bevy::ui::FlexDirection::Row)
            .count();
        assert!(
            split_count >= 1,
            "spawn_split should produce a Pane+PaneSplit with Node{{flex_direction: Row}}"
        );
    }

    #[test]
    fn new_split_wraps_existing_pane_without_converting_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let existing_pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(existing_pane)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: None,
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, existing_pane.to_bits())),
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                                ..Default::default()
                            }],
                        },
                        proto::LayoutNode::Pane {
                            id: None,
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: None,
                                url: "https://example.com".into(),
                                kind: "browser".into(),
                                ..Default::default()
                            }],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        let existing: std::collections::HashSet<String> = [
            format_id(NodeKind::Tab, tab.to_bits()),
            format_id(NodeKind::Pane, existing_pane.to_bits()),
            format_id(NodeKind::Stack, stack.to_bits()),
        ]
        .into_iter()
        .collect();

        apply_with_existing(app.world_mut(), &snap, &existing).unwrap();

        assert!(
            app.world().get::<PaneSplit>(existing_pane).is_none(),
            "existing pane should stay a leaf"
        );

        let splits: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .collect();
        assert_eq!(splits.len(), 1, "exactly one new split entity should exist");
        let new_split = splits[0];

        let node = app.world().get::<Node>(new_split).unwrap();
        assert_eq!(node.flex_direction, bevy::ui::FlexDirection::Row);

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(new_split)
            .expect("split has children")
            .iter()
            .collect();
        assert_eq!(children.len(), 2, "split should have two leaf children");
        assert_eq!(
            children[0], existing_pane,
            "existing pane should be first per submitted order"
        );

        let stack_parent = app.world().get::<ChildOf>(stack).map(|p| p.parent());
        assert_eq!(
            stack_parent,
            Some(existing_pane),
            "existing stack should stay under existing pane"
        );
    }

    #[test]
    fn new_root_split_id_none_reuses_existing_root_split_of_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let existing_root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let existing_leaf = app
            .world_mut()
            .spawn((leaf_pane_bundle(), ChildOf(existing_root)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(existing_leaf)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: None,
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, existing_leaf.to_bits())),
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                                ..Default::default()
                            }],
                        },
                        proto::LayoutNode::Pane {
                            id: None,
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: None,
                                url: "https://example.com".into(),
                                kind: "browser".into(),
                                ..Default::default()
                            }],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        let existing: std::collections::HashSet<String> = [
            format_id(NodeKind::Tab, tab.to_bits()),
            format_id(NodeKind::Split, existing_root.to_bits()),
            format_id(NodeKind::Pane, existing_leaf.to_bits()),
            format_id(NodeKind::Stack, stack.to_bits()),
        ]
        .into_iter()
        .collect();

        apply_with_existing(app.world_mut(), &snap, &existing).unwrap();

        let splits: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .collect();
        assert_eq!(
            splits,
            vec![existing_root],
            "should reuse existing root split, not spawn a new one"
        );

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(existing_root)
            .expect("root split has children")
            .iter()
            .collect();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], existing_leaf);
    }

    #[test]
    fn serve_snapshot_requests_emits_response() {
        use bevy::ecs::message::Messages;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LayoutSnapshotRequest>()
            .add_message::<LayoutSnapshotResponse>()
            .insert_resource(crate::stack::FocusedStack::default())
            .add_systems(Update, super::serve_snapshot_requests);

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let _ = app
            .world_mut()
            .spawn((leaf_pane_bundle(), ChildOf(tab)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<LayoutSnapshotRequest>>()
            .write(LayoutSnapshotRequest {
                request_id: [7; 16],
                anchor: None,
            });
        app.update();

        let responses = app.world().resource::<Messages<LayoutSnapshotResponse>>();
        let mut cursor = responses.get_cursor();
        let response = cursor
            .read(responses)
            .next()
            .expect("expected one response");
        assert_eq!(response.request_id, [7; 16]);
        assert_eq!(response.snapshot.tabs.len(), 1);
    }

    #[test]
    fn apply_layout_requests_emits_response_with_snapshot() {
        use bevy::ecs::message::Messages;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LayoutApplyRequest>()
            .add_message::<LayoutApplyResponse>()
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(crate::stack::FocusedStack::default())
            .add_systems(Update, super::apply_layout_requests);

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), ChildOf(tab)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                    is_zoomed: false,
                    stacks: vec![],
                },
            }],
            focused: proto::Focus::default(),
        };

        app.world_mut()
            .resource_mut::<Messages<LayoutApplyRequest>>()
            .write(LayoutApplyRequest {
                request_id: [42; 16],
                snapshot: snap.clone(),
            });
        app.update();

        let responses = app.world().resource::<Messages<LayoutApplyResponse>>();
        let mut cursor = responses.get_cursor();
        let response = cursor
            .read(responses)
            .next()
            .expect("expected one response");
        assert_eq!(response.request_id, [42; 16]);
        assert!(response.result.is_ok(), "apply should succeed");
    }

    #[test]
    fn new_split_preserves_submitted_children_order_with_new_pane_first() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app.world_mut().spawn(LayoutTab { name: "S".into() }).id();
        let existing_pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(existing_pane)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: None,
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: None,
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: None,
                                url: "https://example.com".into(),
                                kind: "browser".into(),
                                ..Default::default()
                            }],
                        },
                        proto::LayoutNode::Pane {
                            id: Some(format_id(NodeKind::Pane, existing_pane.to_bits())),
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                                ..Default::default()
                            }],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        let existing: std::collections::HashSet<String> = [
            format_id(NodeKind::Tab, tab.to_bits()),
            format_id(NodeKind::Pane, existing_pane.to_bits()),
            format_id(NodeKind::Stack, stack.to_bits()),
        ]
        .into_iter()
        .collect();

        apply_with_existing(app.world_mut(), &snap, &existing).unwrap();

        let splits: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .collect();
        let new_split = splits[0];
        let children: Vec<Entity> = app
            .world()
            .get::<Children>(new_split)
            .expect("split has children")
            .iter()
            .collect();
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[1], existing_pane,
            "existing pane should be second per submitted order"
        );
    }
}
