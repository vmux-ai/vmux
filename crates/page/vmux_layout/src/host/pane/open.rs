use crate::{
    stack::{ActiveTabParam, Stack, active_stack_in_pane, focused_stack, stack_bundle},
    tab::Tab,
};
use bevy::{ecs::relationship::Relationship, prelude::*};
use vmux_api::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
use vmux_core::{PageOpenRequest, PageOpenTarget, PageOpenTask};
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use super::{
    OpenRequest, first_stack_in_pane,
    focus::PendingCursorWarp,
    identity::{SpawnCounter, SpawnSeq},
    tree::{
        Pane, PaneSplit, PaneSplitDirection, direction_to_split, first_leaf_descendant,
        split_leaf_into_two, split_or_extend,
    },
};
use crate::host::command::LayoutRequestSet;

pub(super) struct OpenPlugin;

impl Plugin for OpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((BesideOpenPlugin, DirectionalOpenPlugin));
    }
}

pub(super) struct BesideOpenPlugin;

impl Plugin for BesideOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenBesideRequest>().add_systems(
            Update,
            handle_open_beside_requests.in_set(LayoutRequestSet::Handle),
        );
    }
}

pub(super) struct DirectionalOpenPlugin;

impl Plugin for DirectionalOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenRequest>()
            .add_systems(Update, handle_open_in_pane.in_set(LayoutRequestSet::Handle));
    }
}

#[derive(Message, Clone)]
pub struct OpenBesideRequest {
    pub pane: Entity,
    pub direction: Option<PaneDirection>,
    pub url: String,
    pub request_id: [u8; 16],
    pub focus: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
struct ResolverCtx<'w, 's> {
    all_children: Query<'w, 's, &'static Children>,
    seq_q: Query<'w, 's, &'static SpawnSeq>,
    node_q: Query<'w, 's, &'static ComputedNode>,
    page_q: Query<'w, 's, &'static vmux_core::PageMetadata, With<Stack>>,
    open_task_q: Query<'w, 's, &'static PageOpenTask>,
    spaces: Query<'w, 's, (), With<crate::space::Space>>,
    tab_q: Query<'w, 's, Entity, With<Tab>>,
}

fn handle_open_beside_requests(
    mut reader: MessageReader<OpenBesideRequest>,
    pane_children: Query<&Children, With<Pane>>,
    split_dir_q: Query<&PaneSplit>,
    tab_filter: Query<Entity, With<Stack>>,
    child_of_q: Query<&ChildOf>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    rc: ResolverCtx,
    mut commands: Commands,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut spawn_counter: ResMut<SpawnCounter>,
) {
    let mut split_this_batch: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    let mut spawn_seq_overrides: std::collections::HashMap<Entity, u64> =
        std::collections::HashMap::new();
    let mut pending_leaf_infos: std::collections::HashMap<Entity, crate::placement::LeafInfo> =
        std::collections::HashMap::new();
    let mut pending_leaf_stacks: std::collections::HashMap<Entity, Vec<Entity>> =
        std::collections::HashMap::new();
    let mut pending_open_stacks: Vec<(String, Entity)> = Vec::new();
    let mut retired_leaf_panes: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for req in reader.read() {
        let reuse = crate::space::space_of(req.pane, &child_of_q, &rc.spaces).and_then(|space| {
            find_reuse_in_space(
                &req.url,
                space,
                &rc.tab_q,
                &rc.all_children,
                &rc.page_q,
                &rc.open_task_q,
                &child_of_q,
            )
        });
        if let Some(hit) = reuse {
            if let Ok(meta) = rc.page_q.get(hit.stack)
                && meta.url != req.url
            {
                page_open_requests.write(PageOpenRequest {
                    target: PageOpenTarget::Stack(hit.stack),
                    url: req.url.clone(),
                    request_id: None,
                });
            }
            if req.focus {
                focus_reuse_hit(&mut commands, &child_of_q, hit);
            }
            continue;
        }
        if req.direction.is_none()
            && let Some(index) = pending_open_match_index(&req.url, &pending_open_stacks)
        {
            let (pending_url, stack) = &mut pending_open_stacks[index];
            if *pending_url != req.url {
                page_open_requests.write(PageOpenRequest {
                    target: PageOpenTarget::Stack(*stack),
                    url: req.url.clone(),
                    request_id: None,
                });
                *pending_url = req.url.clone();
            }
            if req.focus {
                focus_stack_in_layout(&mut commands, &child_of_q, &rc.tab_q, *stack);
            }
            continue;
        }

        if let Some(direction) = req.direction {
            let (target_pane, pending_size, refresh_spawn_seq) = match find_sibling_pane(
                req.pane,
                &direction,
                &child_of_q,
                &split_dir_q,
                &pane_children,
                &leaf_panes,
            ) {
                Some(sibling) => (sibling, pane_size(sibling, &rc.node_q), false),
                None => {
                    let existing_tabs = stack_children_for_split(
                        req.pane,
                        &pane_children,
                        &tab_filter,
                        &pending_leaf_stacks,
                    );
                    let old_leaf_info = leaf_info_for_pane(
                        req.pane,
                        &pane_children,
                        &rc.seq_q,
                        &rc.node_q,
                        &rc.page_q,
                        &spawn_seq_overrides,
                    );
                    let split_dir = direction_to_split(&direction);
                    let already_split =
                        !split_this_batch.insert(req.pane) || split_dir_q.contains(req.pane);
                    let split = split_or_extend_for_batch(
                        &mut commands,
                        req.pane,
                        split_dir,
                        &existing_tabs,
                        req.focus,
                        already_split,
                        old_leaf_info,
                        &mut pending_leaf_infos,
                        &mut pending_leaf_stacks,
                        &mut retired_leaf_panes,
                    );
                    stamp_split_panes_for_batch(
                        &mut commands,
                        &mut spawn_counter,
                        &rc.seq_q,
                        &mut spawn_seq_overrides,
                        &mut pending_leaf_infos,
                        split.holder,
                        split.target,
                    );
                    let pending_size = split
                        .target_size
                        .unwrap_or_else(|| pane_size(split.target, &rc.node_q));
                    (split.target, pending_size, false)
                }
            };
            let stack = spawn_beside_stack(
                target_pane,
                req,
                &mut commands,
                &mut page_open_requests,
                &mut spawn_counter,
                &rc.seq_q,
                &mut spawn_seq_overrides,
                &mut pending_leaf_infos,
                &mut pending_leaf_stacks,
                pending_size,
                refresh_spawn_seq,
            );
            pending_open_stacks.push((req.url.clone(), stack));
            continue;
        }

        let Some(tab) = tab_of_pane(req.pane, &child_of_q, &rc.tab_q) else {
            let stack = spawn_beside_stack(
                req.pane,
                req,
                &mut commands,
                &mut page_open_requests,
                &mut spawn_counter,
                &rc.seq_q,
                &mut spawn_seq_overrides,
                &mut pending_leaf_infos,
                &mut pending_leaf_stacks,
                pane_size(req.pane, &rc.node_q),
                false,
            );
            pending_open_stacks.push((req.url.clone(), stack));
            continue;
        };
        let mut leaves = collect_leaf_infos(
            tab,
            &rc.all_children,
            &leaf_panes,
            &pane_children,
            &rc.seq_q,
            &rc.node_q,
            &rc.page_q,
            &spawn_seq_overrides,
        );
        leaves.retain(|leaf| !retired_leaf_panes.contains(&leaf.pane));
        merge_pending_leaf_infos(&mut leaves, &pending_leaf_infos);

        match crate::placement::resolve_placement(&req.url, reuse, &leaves, req.pane) {
            crate::placement::Placement::Focus { tab, stack } => {
                focus_reuse_hit(
                    &mut commands,
                    &child_of_q,
                    crate::placement::ReuseHit { tab, stack },
                );
            }
            crate::placement::Placement::AddTab { pane } => {
                let refresh_spawn_seq = matches!(
                    crate::placement::page_kind_for_url(&req.url),
                    crate::placement::PageKind::File | crate::placement::PageKind::Terminal
                );
                let stack = spawn_beside_stack(
                    pane,
                    req,
                    &mut commands,
                    &mut page_open_requests,
                    &mut spawn_counter,
                    &rc.seq_q,
                    &mut spawn_seq_overrides,
                    &mut pending_leaf_infos,
                    &mut pending_leaf_stacks,
                    pane_size(pane, &rc.node_q),
                    refresh_spawn_seq,
                );
                pending_open_stacks.push((req.url.clone(), stack));
            }
            crate::placement::Placement::Spiral { anchor, axis } => {
                let old_leaf_info = leaves.iter().find(|leaf| leaf.pane == anchor).cloned();
                let existing_tabs = stack_children_for_split(
                    anchor,
                    &pane_children,
                    &tab_filter,
                    &pending_leaf_stacks,
                );
                let already_split =
                    !split_this_batch.insert(anchor) || split_dir_q.contains(anchor);
                let split = split_or_extend_for_batch(
                    &mut commands,
                    anchor,
                    axis,
                    &existing_tabs,
                    req.focus,
                    already_split,
                    old_leaf_info,
                    &mut pending_leaf_infos,
                    &mut pending_leaf_stacks,
                    &mut retired_leaf_panes,
                );
                stamp_split_panes_for_batch(
                    &mut commands,
                    &mut spawn_counter,
                    &rc.seq_q,
                    &mut spawn_seq_overrides,
                    &mut pending_leaf_infos,
                    split.holder,
                    split.target,
                );
                let pending_size = split
                    .target_size
                    .unwrap_or_else(|| pane_size(anchor, &rc.node_q));
                let stack = spawn_beside_stack(
                    split.target,
                    req,
                    &mut commands,
                    &mut page_open_requests,
                    &mut spawn_counter,
                    &rc.seq_q,
                    &mut spawn_seq_overrides,
                    &mut pending_leaf_infos,
                    &mut pending_leaf_stacks,
                    pending_size,
                    false,
                );
                pending_open_stacks.push((req.url.clone(), stack));
            }
        }
    }
}

struct BatchSplit {
    target: Entity,
    holder: Option<Entity>,
    target_size: Option<Vec2>,
}

fn split_or_extend_for_batch(
    commands: &mut Commands,
    anchor: Entity,
    split_dir: PaneSplitDirection,
    existing_tabs: &[Entity],
    activate_new: bool,
    already_split: bool,
    old_leaf_info: Option<crate::placement::LeafInfo>,
    pending_leaf_infos: &mut std::collections::HashMap<Entity, crate::placement::LeafInfo>,
    pending_leaf_stacks: &mut std::collections::HashMap<Entity, Vec<Entity>>,
    retired_leaf_panes: &mut std::collections::HashSet<Entity>,
) -> BatchSplit {
    if already_split {
        return BatchSplit {
            target: split_or_extend(
                commands,
                anchor,
                split_dir,
                existing_tabs,
                activate_new,
                true,
            ),
            holder: None,
            target_size: None,
        };
    }

    let pending_info = pending_leaf_infos.remove(&anchor);
    pending_leaf_stacks.remove(&anchor);
    let (holder, target) =
        PaneSplit::spawn_from_leaf(commands, anchor, split_dir, existing_tabs, activate_new);
    retired_leaf_panes.insert(anchor);
    let target_size = pending_info
        .as_ref()
        .or(old_leaf_info.as_ref())
        .map(|info| split_child_size(info.size, split_dir));
    if let Some(mut info) = pending_info.or(old_leaf_info) {
        info.pane = holder;
        info.size = split_child_size(info.size, split_dir);
        pending_leaf_infos.insert(holder, info);
    }
    if !existing_tabs.is_empty() {
        pending_leaf_stacks.insert(holder, existing_tabs.to_vec());
    }
    BatchSplit {
        target,
        holder: Some(holder),
        target_size,
    }
}

#[allow(clippy::too_many_arguments)]
fn stamp_split_panes_for_batch(
    commands: &mut Commands,
    spawn_counter: &mut SpawnCounter,
    seq_q: &Query<&SpawnSeq>,
    spawn_seq_overrides: &mut std::collections::HashMap<Entity, u64>,
    pending_leaf_infos: &mut std::collections::HashMap<Entity, crate::placement::LeafInfo>,
    holder: Option<Entity>,
    target: Entity,
) {
    let mut stamp = |pane| {
        let seq = touch_pane_spawn_seq(pane, commands, spawn_counter, seq_q);
        spawn_seq_overrides.insert(pane, seq.0);
        if let Some(info) = pending_leaf_infos.get_mut(&pane) {
            info.spawn_seq = seq.0;
        }
    };
    if let Some(holder) = holder {
        stamp(holder);
        stamp(target);
    } else {
        stamp(target);
    }
}

fn focus_reuse_hit(
    commands: &mut Commands,
    child_of_q: &Query<&ChildOf>,
    hit: crate::placement::ReuseHit,
) {
    if let Ok(co) = child_of_q.get(hit.stack) {
        commands.entity(co.get()).insert(LastActivatedAt::now());
    }
    commands.entity(hit.stack).insert(LastActivatedAt::now());
    commands.entity(hit.tab).insert(LastActivatedAt::now());
}

fn focus_stack_in_layout(
    commands: &mut Commands,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<Tab>>,
    stack: Entity,
) {
    if let Ok(co) = child_of_q.get(stack) {
        let pane = co.get();
        commands.entity(pane).insert(LastActivatedAt::now());
        if let Some(tab) = tab_of_pane(pane, child_of_q, tab_q) {
            commands.entity(tab).insert(LastActivatedAt::now());
        }
    }
    commands.entity(stack).insert(LastActivatedAt::now());
}

fn touch_pane_spawn_seq(
    target_pane: Entity,
    commands: &mut Commands,
    spawn_counter: &mut SpawnCounter,
    seq_q: &Query<&SpawnSeq>,
) -> SpawnSeq {
    let max_existing = seq_q.iter().map(|s| s.0).max().unwrap_or(0);
    if spawn_counter.0 <= max_existing {
        spawn_counter.0 = max_existing;
    }
    spawn_counter.0 += 1;
    let seq = SpawnSeq(spawn_counter.0);
    commands.entity(target_pane).insert(seq);
    seq
}

fn current_pane_spawn_seq(
    pane: Entity,
    seq_q: &Query<&SpawnSeq>,
    spawn_seq_overrides: &std::collections::HashMap<Entity, u64>,
    pending_leaf_infos: &std::collections::HashMap<Entity, crate::placement::LeafInfo>,
) -> u64 {
    pending_leaf_infos
        .get(&pane)
        .map(|info| info.spawn_seq)
        .or_else(|| spawn_seq_overrides.get(&pane).copied())
        .or_else(|| seq_q.get(pane).ok().map(|s| s.0))
        .unwrap_or(0)
}

fn spawn_beside_stack(
    target_pane: Entity,
    req: &OpenBesideRequest,
    commands: &mut Commands,
    page_open_requests: &mut MessageWriter<PageOpenRequest>,
    spawn_counter: &mut SpawnCounter,
    seq_q: &Query<&SpawnSeq>,
    spawn_seq_overrides: &mut std::collections::HashMap<Entity, u64>,
    pending_leaf_infos: &mut std::collections::HashMap<Entity, crate::placement::LeafInfo>,
    pending_leaf_stacks: &mut std::collections::HashMap<Entity, Vec<Entity>>,
    pending_size: Vec2,
    refresh_spawn_seq: bool,
) -> Entity {
    let spawn_seq = if refresh_spawn_seq {
        let seq = touch_pane_spawn_seq(target_pane, commands, spawn_counter, seq_q);
        spawn_seq_overrides.insert(target_pane, seq.0);
        seq.0
    } else {
        current_pane_spawn_seq(target_pane, seq_q, spawn_seq_overrides, pending_leaf_infos)
    };
    record_pending_leaf_info(
        pending_leaf_infos,
        target_pane,
        crate::placement::page_kind_for_url(&req.url),
        spawn_seq,
        pending_size,
    );
    let stack_ts = if req.focus {
        LastActivatedAt::now()
    } else {
        LastActivatedAt(0)
    };
    let new_stack = commands
        .spawn((stack_bundle(), stack_ts, ChildOf(target_pane)))
        .id();
    commands.entity(new_stack).insert(vmux_core::PageMetadata {
        url: req.url.clone(),
        ..default()
    });
    pending_leaf_stacks
        .entry(target_pane)
        .or_default()
        .push(new_stack);
    open_stack(
        new_stack,
        req.url.clone(),
        (!req.url.starts_with("file:") && vmux_api::VmuxRoute::parse(&req.url).is_none())
            .then_some(req.request_id),
        page_open_requests,
    );
    new_stack
}

fn pending_open_match_index(url: &str, pending_open_stacks: &[(String, Entity)]) -> Option<usize> {
    pending_open_stacks
        .iter()
        .position(|(pending_url, _)| crate::placement::reusable_page_match(url, pending_url))
}

fn pane_size(pane: Entity, node_q: &Query<&ComputedNode>) -> Vec2 {
    node_q.get(pane).map(|n| n.size).unwrap_or(Vec2::ZERO)
}

fn split_child_size(size: Vec2, split_dir: PaneSplitDirection) -> Vec2 {
    match split_dir {
        PaneSplitDirection::Row => Vec2::new(size.x * 0.5, size.y),
        PaneSplitDirection::Column => Vec2::new(size.x, size.y * 0.5),
    }
}

fn record_pending_leaf_info(
    pending_leaf_infos: &mut std::collections::HashMap<Entity, crate::placement::LeafInfo>,
    pane: Entity,
    kind: crate::placement::PageKind,
    spawn_seq: u64,
    size: Vec2,
) {
    let info = pending_leaf_infos
        .entry(pane)
        .or_insert_with(|| crate::placement::LeafInfo {
            pane,
            kinds: Vec::new(),
            spawn_seq,
            size,
        });
    if !info.kinds.contains(&kind) {
        info.kinds.push(kind);
    }
    info.spawn_seq = spawn_seq;
    if info.size == Vec2::ZERO {
        info.size = size;
    }
}

fn merge_pending_leaf_infos(
    leaves: &mut Vec<crate::placement::LeafInfo>,
    pending_leaf_infos: &std::collections::HashMap<Entity, crate::placement::LeafInfo>,
) {
    for pending in pending_leaf_infos.values() {
        if let Some(existing) = leaves.iter_mut().find(|leaf| leaf.pane == pending.pane) {
            for kind in &pending.kinds {
                if !existing.kinds.contains(kind) {
                    existing.kinds.push(*kind);
                }
            }
            existing.spawn_seq = pending.spawn_seq;
            if existing.size == Vec2::ZERO {
                existing.size = pending.size;
            }
        } else {
            leaves.push(pending.clone());
        }
    }
}

fn stack_children_for_split(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    tab_filter: &Query<Entity, With<Stack>>,
    pending_leaf_stacks: &std::collections::HashMap<Entity, Vec<Entity>>,
) -> Vec<Entity> {
    let mut stacks: Vec<Entity> = pane_children
        .get(pane)
        .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
        .unwrap_or_default();
    if let Some(pending) = pending_leaf_stacks.get(&pane) {
        for &stack in pending {
            if !stacks.contains(&stack) {
                stacks.push(stack);
            }
        }
    }
    stacks
}

fn leaf_info_for_pane(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    seq_q: &Query<&SpawnSeq>,
    node_q: &Query<&ComputedNode>,
    page_q: &Query<&vmux_core::PageMetadata, With<Stack>>,
    spawn_seq_overrides: &std::collections::HashMap<Entity, u64>,
) -> Option<crate::placement::LeafInfo> {
    let kinds = unique_page_kinds(
        pane_children
            .get(pane)
            .ok()?
            .iter()
            .filter_map(|child| page_q.get(child).ok())
            .map(|p| p.url.as_str()),
    );
    Some(crate::placement::LeafInfo {
        pane,
        kinds,
        spawn_seq: spawn_seq_overrides
            .get(&pane)
            .copied()
            .or_else(|| seq_q.get(pane).ok().map(|s| s.0))
            .unwrap_or(0),
        size: node_q.get(pane).map(|n| n.size).unwrap_or(Vec2::ZERO),
    })
}

fn tab_of_pane(
    pane: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<Tab>>,
) -> Option<Entity> {
    let mut cur = pane;
    for _ in 0..32 {
        if tab_q.contains(cur) {
            return Some(cur);
        }
        cur = child_of_q.get(cur).ok()?.get();
    }
    None
}

fn collect_leaf_infos(
    tab: Entity,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: &Query<&Children, With<Pane>>,
    seq_q: &Query<&SpawnSeq>,
    node_q: &Query<&ComputedNode>,
    page_q: &Query<&vmux_core::PageMetadata, With<Stack>>,
    spawn_seq_overrides: &std::collections::HashMap<Entity, u64>,
) -> Vec<crate::placement::LeafInfo> {
    let mut panes = Vec::new();
    crate::stack::collect_leaf_panes(tab, all_children, leaf_panes, &mut panes);
    panes
        .into_iter()
        .map(|pane| {
            let kinds = pane_children
                .get(pane)
                .map(|c| {
                    unique_page_kinds(
                        c.iter()
                            .filter_map(|child| page_q.get(child).ok())
                            .map(|p| p.url.as_str()),
                    )
                })
                .unwrap_or_default();
            crate::placement::LeafInfo {
                pane,
                kinds,
                spawn_seq: spawn_seq_overrides
                    .get(&pane)
                    .copied()
                    .or_else(|| seq_q.get(pane).ok().map(|s| s.0))
                    .unwrap_or(0),
                size: node_q.get(pane).map(|n| n.size).unwrap_or(Vec2::ZERO),
            }
        })
        .collect()
}

fn unique_page_kinds<'a>(urls: impl Iterator<Item = &'a str>) -> Vec<crate::placement::PageKind> {
    let mut kinds = Vec::new();
    for url in urls {
        let kind = crate::placement::page_kind_for_url(url);
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds
}

fn find_reuse_in_space(
    url: &str,
    space: Entity,
    tab_q: &Query<Entity, With<Tab>>,
    all_children: &Query<&Children>,
    page_q: &Query<&vmux_core::PageMetadata, With<Stack>>,
    open_task_q: &Query<&PageOpenTask>,
    child_of_q: &Query<&ChildOf>,
) -> Option<crate::placement::ReuseHit> {
    let tabs: Vec<Entity> = all_children
        .get(space)
        .map(|c| c.iter().filter(|&e| tab_q.contains(e)).collect())
        .unwrap_or_default();
    for tab in tabs {
        let mut frontier = vec![tab];
        while let Some(node) = frontier.pop() {
            if let Ok(meta) = page_q.get(node)
                && crate::placement::reusable_page_match(url, &meta.url)
            {
                return Some(crate::placement::ReuseHit { tab, stack: node });
            }
            if let Ok(children) = all_children.get(node) {
                frontier.extend(children.iter());
            }
        }
    }
    for task in open_task_q.iter() {
        if !crate::placement::reusable_page_match(url, &task.url) {
            continue;
        }
        if let Some(tab) = tab_for_stack_in_space(task.stack, space, child_of_q, tab_q) {
            return Some(crate::placement::ReuseHit {
                tab,
                stack: task.stack,
            });
        }
    }
    None
}

fn tab_for_stack_in_space(
    stack: Entity,
    space: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<Tab>>,
) -> Option<Entity> {
    let mut cur = stack;
    let mut tab = None;
    for _ in 0..32 {
        if tab_q.contains(cur) {
            tab = Some(cur);
        }
        if cur == space {
            return tab;
        }
        cur = child_of_q.get(cur).ok()?.get();
    }
    None
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct PlacementCtx<'w, 's> {
    pub child_of_q: Query<'w, 's, &'static ChildOf>,
    pub tab_q: Query<'w, 's, Entity, With<Tab>>,
    pub all_children: Query<'w, 's, &'static Children>,
    pub leaf_panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    pub pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    pub split_dir_q: Query<'w, 's, &'static PaneSplit>,
    pub tab_filter: Query<'w, 's, Entity, With<Stack>>,
    pub seq_q: Query<'w, 's, &'static SpawnSeq>,
    pub node_q: Query<'w, 's, &'static ComputedNode>,
    pub page_q: Query<'w, 's, &'static vmux_core::PageMetadata, With<Stack>>,
}

pub fn resolve_spiral_pane(
    commands: &mut Commands,
    anchor_pane: Entity,
    url: &str,
    focus: bool,
    split_batch: &mut std::collections::HashSet<Entity>,
    ctx: &PlacementCtx,
) -> Entity {
    let Some(tab) = tab_of_pane(anchor_pane, &ctx.child_of_q, &ctx.tab_q) else {
        return anchor_pane;
    };
    let leaves = collect_leaf_infos(
        tab,
        &ctx.all_children,
        &ctx.leaf_panes,
        &ctx.pane_children,
        &ctx.seq_q,
        &ctx.node_q,
        &ctx.page_q,
        &std::collections::HashMap::new(),
    );
    match crate::placement::resolve_placement(url, None, &leaves, anchor_pane) {
        crate::placement::Placement::AddTab { pane } => pane,
        crate::placement::Placement::Spiral { anchor, axis } => {
            let existing_tabs: Vec<Entity> = ctx
                .pane_children
                .get(anchor)
                .map(|c| c.iter().filter(|&e| ctx.tab_filter.contains(e)).collect())
                .unwrap_or_default();
            let already_split = !split_batch.insert(anchor) || ctx.split_dir_q.contains(anchor);
            split_or_extend(commands, anchor, axis, &existing_tabs, focus, already_split)
        }
        crate::placement::Placement::Focus { .. } => anchor_pane,
    }
}

pub fn resolve_split_anchor_pane(anchor_pane: Entity, ctx: &PlacementCtx) -> Entity {
    let Some(tab) = tab_of_pane(anchor_pane, &ctx.child_of_q, &ctx.tab_q) else {
        return anchor_pane;
    };
    let leaves = collect_leaf_infos(
        tab,
        &ctx.all_children,
        &ctx.leaf_panes,
        &ctx.pane_children,
        &ctx.seq_q,
        &ctx.node_q,
        &ctx.page_q,
        &std::collections::HashMap::new(),
    );
    crate::placement::resolve_split_anchor(&leaves, anchor_pane)
}

fn is_after_direction(direction: &PaneDirection) -> bool {
    matches!(direction, PaneDirection::Right | PaneDirection::Bottom)
}

fn find_sibling_pane(
    active: Entity,
    direction: &PaneDirection,
    child_of_q: &Query<&ChildOf>,
    split_dir_q: &Query<&PaneSplit>,
    pane_children: &Query<&Children, With<Pane>>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Option<Entity> {
    let target_split = direction_to_split(direction);
    let after = is_after_direction(direction);

    let mut cur = active;
    for _ in 0..20 {
        let Ok(co) = child_of_q.get(cur) else {
            return None;
        };
        let parent = co.get();
        let Ok(ps) = split_dir_q.get(parent) else {
            cur = parent;
            continue;
        };
        if ps.direction != target_split {
            cur = parent;
            continue;
        }
        let Ok(children) = pane_children.get(parent) else {
            cur = parent;
            continue;
        };
        let sibs: Vec<Entity> = children.iter().collect();
        let Some(idx) = sibs.iter().position(|&e| e == cur) else {
            cur = parent;
            continue;
        };
        let sibling_idx = if after { idx + 1 } else { idx.wrapping_sub(1) };
        let sibling = sibs.get(sibling_idx).copied()?;
        return Some(first_leaf_descendant(sibling, pane_children, leaf_panes));
    }
    None
}

fn handle_open_in_pane(
    mut reader: MessageReader<OpenRequest>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    child_of_q: Query<&ChildOf>,
    split_dir_q: Query<&PaneSplit>,
    tab_filter: Query<Entity, With<Stack>>,
    effective_startup_url: Option<Res<vmux_core::EffectiveStartupUrl>>,
    mut commands: Commands,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut pending_warp: ResMut<PendingCursorWarp>,
) {
    for request in reader.read() {
        let OpenRequest {
            direction,
            target,
            mode,
            url,
        } = request;

        let (_, active_pane_opt, _) = focused_stack(
            active_tab_param.get(),
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let Some(active) = active_pane_opt else {
            continue;
        };

        let resolved = url
            .clone()
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| {
                vmux_core::EffectiveStartupUrl::resolve(effective_startup_url.as_deref())
            });

        let split_dir = direction_to_split(direction);

        let (target_pane, was_split) = match target {
            PaneTarget::Existing => {
                match find_sibling_pane(
                    active,
                    direction,
                    &child_of_q,
                    &split_dir_q,
                    &pane_children,
                    &leaf_panes,
                ) {
                    Some(sibling) => (sibling, false),
                    None => {
                        let existing_tabs: Vec<Entity> = pane_children
                            .get(active)
                            .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                            .unwrap_or_default();
                        let p2 = split_leaf_into_two(
                            &mut commands,
                            active,
                            split_dir,
                            &existing_tabs,
                            true,
                        );
                        (p2, true)
                    }
                }
            }
            PaneTarget::NewSplit => {
                let existing_tabs: Vec<Entity> = pane_children
                    .get(active)
                    .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                    .unwrap_or_default();
                let p2 =
                    split_leaf_into_two(&mut commands, active, split_dir, &existing_tabs, true);
                (p2, true)
            }
        };

        if was_split {
            let new_stack = commands
                .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(target_pane)))
                .id();
            open_stack(new_stack, resolved, None, &mut page_open_requests);
        } else {
            match mode {
                PaneOpenMode::InPlace => {
                    let active_stack = active_stack_in_pane(target_pane, &pane_children, &stack_ts)
                        .or_else(|| first_stack_in_pane(target_pane, &pane_children, &tab_filter));
                    if let Some(stack) = active_stack {
                        open_stack(stack, resolved, None, &mut page_open_requests);
                    }
                }
                PaneOpenMode::NewStack => {
                    let new_stack = commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(target_pane)))
                        .id();
                    open_stack(new_stack, resolved, None, &mut page_open_requests);
                }
            }
        }
        pending_warp.target = Some(target_pane);
    }
}

fn open_stack(
    stack: Entity,
    url: String,
    request_id: Option<[u8; 16]>,
    page_open_requests: &mut MessageWriter<PageOpenRequest>,
) {
    page_open_requests.write(PageOpenRequest {
        target: PageOpenTarget::Stack(stack),
        url,
        request_id,
    });
}
