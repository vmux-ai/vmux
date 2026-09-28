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
use crate::{TerminalLayoutSpawnRequest, event::PANE_GAP_PX};
#[cfg(test)]
use bevy::ecs::message::Messages;
use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_flex::prelude::*;
use vmux_history::{CreatedAt, LastActivatedAt};

pub(super) struct LayoutApplyPlugin;

impl Plugin for LayoutApplyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LayoutApplyPlan>()
            .add_message::<LayoutApplyResult>()
            .add_systems(
                Update,
                (
                    plan_layout_requests,
                    apply_layout_plans,
                    respond_to_layout_apply,
                )
                    .chain(),
            )
            .add_systems(Update, serve_snapshot_requests);
    }
}

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

#[derive(Message)]
struct LayoutApplyPlan {
    request_id: [u8; 16],
    snapshot: LayoutSnapshot,
    diff: DiffPlan,
}

#[derive(Message)]
struct LayoutApplyResult {
    request_id: [u8; 16],
    result: Result<(), String>,
}

fn serve_snapshot_requests(
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

fn plan_layout_requests(
    mut reader: MessageReader<LayoutApplyRequest>,
    active_space_q: Query<Entity, (With<crate::space::Space>, With<vmux_core::Active>)>,
    tabs_q: Query<(Entity, Option<&ChildOf>), With<LayoutTab>>,
    nodes_q: Query<(
        Option<&Children>,
        Has<LayoutTab>,
        Has<PaneSplit>,
        Has<Pane>,
        Has<Stack>,
    )>,
    mut plans: MessageWriter<LayoutApplyPlan>,
    mut results: MessageWriter<LayoutApplyResult>,
) {
    for request in reader.read() {
        let existing = collect_existing_ids(&active_space_q, &tabs_q, &nodes_q);
        match plan_diff(&request.snapshot, &existing) {
            Ok(diff) => {
                plans.write(LayoutApplyPlan {
                    request_id: request.request_id,
                    snapshot: request.snapshot.clone(),
                    diff,
                });
            }
            Err(error) => {
                results.write(LayoutApplyResult {
                    request_id: request.request_id,
                    result: Err(format!("update_layout: {error:?}")),
                });
            }
        }
    }
}

fn apply_layout_plans(
    mut plans: MessageReader<LayoutApplyPlan>,
    children: Query<&Children>,
    child_of: Query<&ChildOf>,
    activated: Query<&LastActivatedAt>,
    mut tabs: Query<&mut LayoutTab>,
    mut splits: Query<(Entity, &mut PaneSplit, Option<&mut Node>)>,
    mut pane_sizes: Query<&mut PaneSize>,
    mut metadata: Query<&mut PageMetadata>,
    mut focused: ResMut<crate::stack::FocusedStack>,
    mut terminal_spawn: MessageWriter<TerminalLayoutSpawnRequest>,
    mut page_open: MessageWriter<PageOpenRequest>,
    mut results: MessageWriter<LayoutApplyResult>,
    mut commands: Commands,
) {
    for plan in plans.read() {
        apply_layout_plan(
            &plan.snapshot,
            &plan.diff,
            &children,
            &child_of,
            &activated,
            &mut tabs,
            &mut splits,
            &mut pane_sizes,
            &mut metadata,
            &mut focused,
            &mut terminal_spawn,
            &mut page_open,
            &mut commands,
        );
        results.write(LayoutApplyResult {
            request_id: plan.request_id,
            result: Ok(()),
        });
    }
}

fn respond_to_layout_apply(
    mut reader: MessageReader<LayoutApplyResult>,
    tabs_q: Query<(Entity, &LayoutTab, Option<&Children>)>,
    splits_q: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: Query<(Entity, Option<&Children>, Option<&vmux_core::PageMetadata>), With<Stack>>,
    pane_sizes_q: Query<&PaneSize>,
    zoomed_q: Query<&crate::pane::Zoomed>,
    focused: Res<crate::stack::FocusedStack>,
    mut writer: MessageWriter<LayoutApplyResponse>,
) {
    for result in reader.read() {
        let response = match &result.result {
            Ok(()) => Ok(crate::snapshot::build_layout_snapshot(
                &tabs_q,
                &splits_q,
                &leaves_q,
                &stacks_q,
                &pane_sizes_q,
                &zoomed_q,
                &focused,
                None,
            )),
            Err(error) => Err(error.clone()),
        };
        writer.write(LayoutApplyResponse {
            request_id: result.request_id,
            result: response,
        });
    }
}

fn apply_layout_plan(
    snapshot: &LayoutSnapshot,
    plan: &DiffPlan,
    children_q: &Query<&Children>,
    child_of: &Query<&ChildOf>,
    activated: &Query<&LastActivatedAt>,
    tabs: &mut Query<&mut LayoutTab>,
    splits: &mut Query<(Entity, &mut PaneSplit, Option<&mut Node>)>,
    pane_sizes: &mut Query<&mut PaneSize>,
    metadata: &mut Query<&mut PageMetadata>,
    focused: &mut crate::stack::FocusedStack,
    terminal_spawn: &mut MessageWriter<TerminalLayoutSpawnRequest>,
    page_open: &mut MessageWriter<PageOpenRequest>,
    commands: &mut Commands,
) {
    let mut new_entities: std::collections::HashMap<*const proto::LayoutNode, Entity> =
        std::collections::HashMap::new();
    let mut materialized: Vec<(&proto::Tab, Entity, i64)> = Vec::with_capacity(snapshot.tabs.len());
    let tab_parent: Option<Entity> = snapshot
        .tabs
        .iter()
        .filter_map(|t| t.id.as_deref())
        .filter_map(|id| parse_id(id).ok())
        .map(|(_, v)| Entity::from_bits(v))
        .find_map(|entity| child_of.get(entity).ok().map(|parent| parent.parent()));
    for tab in &snapshot.tabs {
        let (tab_entity, activated_at) = match &tab.id {
            Some(id) => match parse_id(id) {
                Ok((_, value)) => {
                    let entity = Entity::from_bits(value);
                    let activated_at = activated.get(entity).map_or(0, |value| value.0);
                    (entity, activated_at)
                }
                Err(_) => continue,
            },
            None => {
                let activated_at = LastActivatedAt::now();
                let entity = commands
                    .spawn((crate::tab::tab_bundle(), activated_at, CreatedAt::now()))
                    .id();
                if let Some(parent) = tab_parent {
                    commands.entity(entity).insert(ChildOf(parent));
                }
                if !tab.name.is_empty() {
                    commands.entity(entity).insert(LayoutTab {
                        name: tab.name.clone(),
                        ..default()
                    });
                }
                (entity, activated_at.0)
            }
        };
        materialize_descendants(
            tab_entity,
            true,
            &tab.root,
            &mut new_entities,
            children_q,
            splits,
            terminal_spawn,
            page_open,
            commands,
        );
        materialized.push((tab, tab_entity, activated_at));
    }

    for (tab, tab_entity, _) in &materialized {
        apply_structure(Some(*tab_entity), &tab.root, &new_entities, commands);
    }
    for tab in &snapshot.tabs {
        apply_tab(tab, tabs, splits, pane_sizes, metadata);
    }
    if let Some((_, active_entity, _)) = materialized.iter().find(|(tab, _, _)| tab.is_active) {
        let newest = materialized
            .iter()
            .map(|(_, _, activated_at)| *activated_at)
            .max()
            .unwrap_or(0);
        commands
            .entity(*active_entity)
            .insert(LastActivatedAt(newest + 1));
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
            plan.closes.contains(&id).then_some(id)
        })
        .collect();
    for id in &plan.closes {
        if rescued.contains(id) {
            continue;
        }
        apply_close(id, commands);
    }
    apply_focus(focused, &snapshot.focused);
}

fn materialize_descendants(
    parent: Entity,
    parent_is_tab: bool,
    node: &proto::LayoutNode,
    new_entities: &mut std::collections::HashMap<*const proto::LayoutNode, Entity>,
    children_q: &Query<&Children>,
    splits: &mut Query<(Entity, &mut PaneSplit, Option<&mut Node>)>,
    terminal_spawn: &mut MessageWriter<TerminalLayoutSpawnRequest>,
    page_open: &mut MessageWriter<PageOpenRequest>,
    commands: &mut Commands,
) {
    let node_entity = match node {
        proto::LayoutNode::Split { id, direction, .. } => match id {
            Some(id_str) => match parse_id(id_str) {
                Ok((_, v)) => Entity::from_bits(v),
                Err(_) => return,
            },
            None => {
                if parent_is_tab
                    && let Some(existing_root) = find_root_split_child(children_q, splits, parent)
                {
                    set_split_direction(splits, existing_root, *direction);
                    new_entities.insert(node as *const _, existing_root);
                    existing_root
                } else {
                    let pane_split_dir = match direction {
                        proto::SplitDirection::Row => PaneSplitDirection::Row,
                        proto::SplitDirection::Column => PaneSplitDirection::Column,
                    };
                    let entity = commands
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
                let entity = commands
                    .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(parent)))
                    .id();
                new_entities.insert(node as *const _, entity);
                entity
            }
        },
    };

    match node {
        proto::LayoutNode::Split { children, .. } => {
            for child in children {
                materialize_descendants(
                    node_entity,
                    false,
                    child,
                    new_entities,
                    children_q,
                    splits,
                    terminal_spawn,
                    page_open,
                    commands,
                );
            }
        }
        proto::LayoutNode::Pane { stacks, .. } => {
            for t in stacks {
                if t.id.is_none() {
                    let stack = commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(node_entity)))
                        .id();
                    match t.kind.as_str() {
                        "terminal" => {
                            terminal_spawn.write(TerminalLayoutSpawnRequest { stack });
                        }
                        _ => {
                            page_open.write(PageOpenRequest {
                                target: PageOpenTarget::Stack(stack),
                                url: t.url.clone(),
                                request_id: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn find_root_split_child(
    children: &Query<&Children>,
    splits: &Query<(Entity, &mut PaneSplit, Option<&mut Node>)>,
    tab: Entity,
) -> Option<Entity> {
    children
        .get(tab)
        .ok()?
        .iter()
        .find(|&entity| splits.contains(entity))
}

fn set_split_direction(
    splits: &mut Query<(Entity, &mut PaneSplit, Option<&mut Node>)>,
    entity: Entity,
    direction: proto::SplitDirection,
) {
    let pane_split_dir = match direction {
        proto::SplitDirection::Row => PaneSplitDirection::Row,
        proto::SplitDirection::Column => PaneSplitDirection::Column,
    };
    if let Ok((_, mut split, node)) = splits.get_mut(entity) {
        split.direction = pane_split_dir;
        if let Some(mut node) = node {
            node.flex_direction = match pane_split_dir {
                PaneSplitDirection::Row => FlexDirection::Row,
                PaneSplitDirection::Column => FlexDirection::Column,
            };
            let gap = pane_split_gaps(pane_split_dir, PANE_GAP_PX);
            node.column_gap = gap.column_gap;
            node.row_gap = gap.row_gap;
        }
    }
}

fn apply_close(id: &str, commands: &mut Commands) {
    let Ok((_kind, value)) = parse_id(id) else {
        return;
    };
    commands.entity(Entity::from_bits(value)).try_despawn();
}

fn collect_ids_recursive(
    entity: Entity,
    nodes_q: &Query<(
        Option<&Children>,
        Has<LayoutTab>,
        Has<PaneSplit>,
        Has<Pane>,
        Has<Stack>,
    )>,
    out: &mut ApplyHashSet<String>,
) {
    let Ok((children, is_tab, is_split, is_pane, is_stack)) = nodes_q.get(entity) else {
        return;
    };
    if is_tab {
        out.insert(format_id(NodeKind::Tab, entity.to_bits()));
    } else if is_split {
        out.insert(format_id(NodeKind::Split, entity.to_bits()));
    } else if is_pane {
        out.insert(format_id(NodeKind::Pane, entity.to_bits()));
    } else if is_stack {
        out.insert(format_id(NodeKind::Stack, entity.to_bits()));
    }
    if let Some(children) = children {
        for child in children.iter() {
            collect_ids_recursive(child, nodes_q, out);
        }
    }
}

fn collect_existing_ids(
    active_space_q: &Query<Entity, (With<crate::space::Space>, With<vmux_core::Active>)>,
    tabs_q: &Query<(Entity, Option<&ChildOf>), With<LayoutTab>>,
    nodes_q: &Query<(
        Option<&Children>,
        Has<LayoutTab>,
        Has<PaneSplit>,
        Has<Pane>,
        Has<Stack>,
    )>,
) -> ApplyHashSet<String> {
    let active_space = active_space_q.iter().next();
    let mut out = ApplyHashSet::new();
    for (tab, child_of) in tabs_q.iter() {
        if active_space.is_some() && child_of.map(|child| child.parent()) != active_space {
            continue;
        }
        collect_ids_recursive(tab, nodes_q, &mut out);
    }
    out
}

fn apply_tab(
    tab: &proto::Tab,
    tabs: &mut Query<&mut LayoutTab>,
    splits: &mut Query<(Entity, &mut PaneSplit, Option<&mut Node>)>,
    pane_sizes: &mut Query<&mut PaneSize>,
    metadata: &mut Query<&mut PageMetadata>,
) {
    if let Some(id) = &tab.id
        && let Ok((_, value)) = parse_id(id)
    {
        let entity = Entity::from_bits(value);
        if let Ok(mut layout_tab) = tabs.get_mut(entity) {
            layout_tab.name = tab.name.clone();
        }
    }
    apply_node(&tab.root, splits, pane_sizes, metadata);
}

fn apply_structure(
    parent: Option<Entity>,
    node: &proto::LayoutNode,
    new_entities: &std::collections::HashMap<*const proto::LayoutNode, Entity>,
    commands: &mut Commands,
) {
    let Some(entity) = resolve_node_entity(node, new_entities) else {
        match node {
            proto::LayoutNode::Split { children, .. } => {
                for c in children {
                    apply_structure(parent, c, new_entities, commands);
                }
            }
            proto::LayoutNode::Pane { .. } => {}
        }
        return;
    };
    if let Some(parent) = parent {
        commands.entity(entity).insert(ChildOf(parent));
    }
    match node {
        proto::LayoutNode::Split { children, .. } => {
            for c in children {
                apply_structure(Some(entity), c, new_entities, commands);
            }
        }
        proto::LayoutNode::Pane { stacks, .. } => {
            for t in stacks {
                if let Some(tid) = t.id.as_deref()
                    && let Ok((_, value)) = parse_id(tid)
                {
                    commands
                        .entity(Entity::from_bits(value))
                        .insert(ChildOf(entity));
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

fn apply_node(
    layout: &proto::LayoutNode,
    splits: &mut Query<(Entity, &mut PaneSplit, Option<&mut Node>)>,
    pane_sizes: &mut Query<&mut PaneSize>,
    metadata: &mut Query<&mut PageMetadata>,
) {
    match layout {
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
                if let Ok((_, mut split, node)) = splits.get_mut(entity) {
                    split.direction = pane_split_dir;
                    if let Some(mut node) = node {
                        node.flex_direction = match pane_split_dir {
                            PaneSplitDirection::Row => FlexDirection::Row,
                            PaneSplitDirection::Column => FlexDirection::Column,
                        };
                        let gap = pane_split_gaps(pane_split_dir, PANE_GAP_PX);
                        node.column_gap = gap.column_gap;
                        node.row_gap = gap.row_gap;
                    }
                }
            }
            if !flex_weights.is_empty() && flex_weights.len() == children.len() {
                for (child_dto, weight) in children.iter().zip(flex_weights.iter()) {
                    if let Some(child_entity) = node_entity(child_dto)
                        && let Ok(mut size) = pane_sizes.get_mut(child_entity)
                    {
                        size.flex_grow = *weight;
                    }
                }
            }
            for c in children {
                apply_node(c, splits, pane_sizes, metadata);
            }
        }
        proto::LayoutNode::Pane { stacks, .. } => {
            for t in stacks {
                if let Some(tid) = &t.id
                    && let Ok((_, value)) = parse_id(tid)
                {
                    let entity = Entity::from_bits(value);
                    if !t.title.is_empty()
                        && let Ok(mut page) = metadata.get_mut(entity)
                    {
                        page.title = t.title.clone();
                    }
                }
            }
        }
    }
}

fn apply_focus(focused: &mut crate::stack::FocusedStack, focus: &proto::Focus) {
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
mod tests {
    use super::*;
    use crate::protocol::{Focus, SplitDirection, Stack as StackDto, Tab as TabDto};
    use std::collections::HashSet;

    struct ApplyHarness;

    impl ApplyHarness {
        fn install(app: &mut App) {
            app.add_message::<LayoutApplyRequest>()
                .add_message::<LayoutApplyResponse>()
                .add_message::<LayoutSnapshotRequest>()
                .add_message::<LayoutSnapshotResponse>()
                .add_message::<crate::TerminalLayoutSpawnRequest>()
                .add_message::<PageOpenRequest>()
                .init_resource::<crate::stack::FocusedStack>()
                .add_plugins(LayoutApplyPlugin);
        }

        fn apply(app: &mut App, snapshot: LayoutSnapshot) -> Result<LayoutSnapshot, String> {
            if !app.world().contains_resource::<Messages<LayoutApplyPlan>>() {
                Self::install(app);
            }
            let request_id = [42; 16];
            app.world_mut()
                .resource_mut::<Messages<LayoutApplyRequest>>()
                .write(LayoutApplyRequest {
                    request_id,
                    snapshot,
                });
            app.update();
            let responses = app.world().resource::<Messages<LayoutApplyResponse>>();
            let mut cursor = responses.get_cursor();
            cursor
                .read(responses)
                .find(|response| response.request_id == request_id)
                .expect("layout apply response")
                .result
                .clone()
        }
    }

    #[test]
    fn apply_closes_are_scoped_to_active_space() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let space_a = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let space_b = app.world_mut().spawn(crate::space::Space).id();
        let tab_a = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), ChildOf(space_a)))
            .id();
        let pane_a = app.world_mut().spawn((Pane, ChildOf(tab_a))).id();
        let tab_b = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), ChildOf(space_b)))
            .id();
        let pane_b = app.world_mut().spawn((Pane, ChildOf(tab_b))).id();
        let snapshot = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(format_id(NodeKind::Tab, tab_a.to_bits())),
                name: String::new(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(format_id(NodeKind::Pane, pane_a.to_bits())),
                    is_zoomed: false,
                    stacks: vec![],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snapshot).unwrap();

        assert!(app.world().get_entity(tab_b).is_ok());
        assert!(app.world().get_entity(pane_b).is_ok());
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
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();
        let updated = app.world().get::<PaneSplit>(split_e).unwrap();
        assert_eq!(updated.direction, PaneSplitDirection::Column);
    }

    #[test]
    fn updating_flex_weights_writes_pane_size_flex_grow() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();
        assert_eq!(app.world().get::<PaneSize>(pane_a).unwrap().flex_grow, 3.0);
        assert_eq!(app.world().get::<PaneSize>(pane_b).unwrap().flex_grow, 1.0);
    }

    #[test]
    fn moves_pane_to_new_parent() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();
        let parent = app.world().get::<ChildOf>(moved).map(|p| p.parent());
        assert_eq!(parent, Some(split_b));
    }

    #[test]
    fn moves_stack_to_new_tab_reparents_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![
                proto::Tab {
                    id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                    name: "S".into(),
                    is_active: false,
                    root: proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
                proto::Tab {
                    id: None,
                    name: "YouTube".into(),
                    is_active: true,
                    root: proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: Some(format_id(NodeKind::Stack, stack.to_bits())),
                            ..Default::default()
                        }],
                    },
                },
            ],
            focused: proto::Focus::default(),
        };
        ApplyHarness::apply(&mut app, snap).unwrap();

        let s_parent = app
            .world()
            .get::<ChildOf>(stack)
            .map(|p| p.parent())
            .expect("moved stack still has a parent");
        assert_ne!(
            s_parent, pane,
            "stack should be reparented out of the original pane"
        );
        let s_grandparent = app.world().get::<ChildOf>(s_parent).map(|p| p.parent());
        assert!(
            s_grandparent.is_some() && s_grandparent != Some(tab),
            "moved stack's pane should live under the new tab, not the original"
        );
    }

    #[test]
    fn snapshot_active_tab_becomes_most_recently_activated() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();
        let active_tab = app
            .world_mut()
            .spawn((
                LayoutTab {
                    name: "A".into(),
                    startup_dir: None,
                },
                LastActivatedAt(1),
            ))
            .id();
        let active_pane = app.world_mut().spawn((Pane, ChildOf(active_tab))).id();

        let snap = LayoutSnapshot {
            tabs: vec![
                proto::Tab {
                    id: Some(format_id(NodeKind::Tab, active_tab.to_bits())),
                    name: "A".into(),
                    is_active: true,
                    root: proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, active_pane.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
                proto::Tab {
                    id: None,
                    name: "New".into(),
                    is_active: false,
                    root: proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: None,
                            url: "https://example.com".into(),
                            kind: "browser".into(),
                            ..Default::default()
                        }],
                    },
                },
            ],
            focused: proto::Focus::default(),
        };
        ApplyHarness::apply(&mut app, snap).unwrap();

        let mut q = app
            .world_mut()
            .query_filtered::<(Entity, &LastActivatedAt), With<LayoutTab>>();
        let ts: Vec<(Entity, i64)> = q.iter(app.world()).map(|(e, l)| (e, l.0)).collect();
        let active_ts = ts
            .iter()
            .find(|(e, _)| *e == active_tab)
            .map(|(_, t)| *t)
            .expect("active tab has a timestamp");
        let max_other = ts
            .iter()
            .filter(|(e, _)| *e != active_tab)
            .map(|(_, t)| *t)
            .max()
            .expect("a new tab exists");
        assert!(
            active_ts > max_other,
            "is_active tab ({active_ts}) must out-rank other tabs ({max_other})"
        );
    }

    #[test]
    fn new_tab_parented_as_sibling_of_existing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();
        let main = app.world_mut().spawn_empty().id();
        let tab = app
            .world_mut()
            .spawn((
                LayoutTab {
                    name: "A".into(),
                    startup_dir: None,
                },
                ChildOf(main),
            ))
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();

        let snap = LayoutSnapshot {
            tabs: vec![
                proto::Tab {
                    id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                    name: "A".into(),
                    is_active: true,
                    root: proto::LayoutNode::Pane {
                        id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
                proto::Tab {
                    id: None,
                    name: "New".into(),
                    is_active: false,
                    root: proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: None,
                            url: "https://example.com".into(),
                            kind: "browser".into(),
                            ..Default::default()
                        }],
                    },
                },
            ],
            focused: proto::Focus::default(),
        };
        ApplyHarness::apply(&mut app, snap).unwrap();

        let mut q = app
            .world_mut()
            .query_filtered::<(Entity, Option<&ChildOf>), With<LayoutTab>>();
        let parent = q
            .iter(app.world())
            .find(|(e, _)| *e != tab)
            .and_then(|(_, c)| c.map(|c| c.parent()))
            .expect("new tab exists with a parent");
        assert_eq!(
            parent, main,
            "new tab should be a sibling of the existing tab (same parent)"
        );
    }

    #[test]
    fn omitting_pane_from_snapshot_closes_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();
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
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        let result = ApplyHarness::apply(&mut app, snap);
        assert!(matches!(result, Err(error) if error.contains("MissingReferencedEntity")));
    }

    #[test]
    fn submitting_new_tab_id_none_spawns_stack_entity() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();

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
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();

        let pane_count_before = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .count();

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
        let _ = ApplyHarness::apply(
            &mut app,
            LayoutSnapshot {
                tabs: vec![TabDto {
                    id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                    name: "S".into(),
                    is_active: true,
                    root: bad_node,
                }],
                focused: Default::default(),
            },
        );

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
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();

        let split_count_before = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .count();

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
        let _ = ApplyHarness::apply(
            &mut app,
            LayoutSnapshot {
                tabs: vec![TabDto {
                    id: Some(format_id(NodeKind::Tab, tab.to_bits())),
                    name: "S".into(),
                    is_active: true,
                    root: bad_node,
                }],
                focused: Default::default(),
            },
        );

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
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();

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

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();
        let focused = app.world().resource::<crate::stack::FocusedStack>();
        assert_eq!(focused.tab, Some(tab));
        assert_eq!(focused.pane, Some(pane_e));
        assert_eq!(focused.stack, Some(stack));
    }

    #[test]
    fn apply_focus_preserves_existing_when_dto_fields_omitted() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(crate::stack::FocusedStack::default());

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();
        let f = app.world().resource::<crate::stack::FocusedStack>();
        assert_eq!(f.tab, Some(tab), "focused.tab must be preserved");
        assert_eq!(f.pane, Some(pane_e), "focused.pane must be preserved");
        assert_eq!(f.stack, Some(stack), "focused.stack must be preserved");
    }

    #[test]
    fn new_split_inserts_node_with_flex_direction() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();

        let split_count = app
            .world_mut()
            .query_filtered::<&Node, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .filter(|node| node.flex_direction == FlexDirection::Row)
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
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();

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
        assert_eq!(node.flex_direction, FlexDirection::Row);

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
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();

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
        app.add_plugins(MinimalPlugins);
        ApplyHarness::install(&mut app);

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        let response = ApplyHarness::apply(&mut app, snap).unwrap();
        assert_eq!(response.tabs.len(), 1);
    }

    #[test]
    fn new_split_preserves_submitted_children_order_with_new_pane_first() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
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

        ApplyHarness::apply(&mut app, snap).unwrap();

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
