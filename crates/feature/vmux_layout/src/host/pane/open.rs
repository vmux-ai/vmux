use crate::{
    placement::{LeafInfo, Placement, ReuseHit},
    stack::{ActiveTabParam, LayoutFocus, Stack},
    tab::Tab,
};
use bevy::{ecs::relationship::Relationship, prelude::*};
use vmux_api::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
use vmux_ecs::page::{PagePlacement, PagePlacementCatalog};
use vmux_ecs::{PageMetadata, PageOpenRequest, PageOpenTarget, PageOpenTask};
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use super::{
    OpenRequest, PaneStacks,
    focus::PendingCursorWarp,
    identity::{SpawnCounter, SpawnSeq},
    tree::{Pane, PaneHierarchy, PaneSplit, PaneSplitDirection, PaneTree},
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
            handle_beside_requests.in_set(LayoutRequestSet::Handle),
        );
    }
}

pub(super) struct DirectionalOpenPlugin;

impl Plugin for DirectionalOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenRequest>()
            .add_systems(Update, handle_in.in_set(LayoutRequestSet::Handle));
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
struct PaneOpenResolver<'w, 's> {
    all_children: Query<'w, 's, &'static Children>,
    seq_q: Query<'w, 's, &'static SpawnSeq>,
    node_q: Query<'w, 's, &'static ComputedNode>,
    page_q: Query<'w, 's, &'static PageMetadata, With<Stack>>,
    open_task_q: Query<'w, 's, &'static PageOpenTask>,
    space_hierarchy: crate::space::SpaceHierarchy<'w, 's>,
    tab_q: Query<'w, 's, Entity, With<Tab>>,
    placements: PagePlacementCatalog<'w, 's>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct PaneOpenWriter<'w, 's> {
    requests: MessageWriter<'w, PageOpenRequest>,
    counter: Single<'w, 's, &'static mut SpawnCounter>,
    sequences: Query<'w, 's, &'static SpawnSeq>,
}

impl PaneOpenWriter<'_, '_> {
    fn touch(&mut self, commands: &mut Commands, pane: Entity) -> SpawnSeq {
        let max_existing = self
            .sequences
            .iter()
            .map(|sequence| sequence.0)
            .max()
            .unwrap_or(0);
        if self.counter.0 <= max_existing {
            self.counter.0 = max_existing;
        }
        self.counter.0 += 1;
        let sequence = SpawnSeq(self.counter.0);
        commands.entity(pane).insert(sequence);
        sequence
    }

    fn open(&mut self, stack: Entity, url: String, request_id: Option<[u8; 16]>) {
        self.requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url,
            request_id,
        });
    }
}

fn handle_beside_requests(
    mut reader: MessageReader<OpenBesideRequest>,
    pane_children: Query<&Children, With<Pane>>,
    split_dir_q: Query<&PaneSplit>,
    tab_filter: Query<Entity, With<Stack>>,
    child_of_q: Query<&ChildOf>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_hierarchy: PaneHierarchy,
    focus: LayoutFocus,
    resolver: PaneOpenResolver,
    mut tree: PaneTree,
    mut writer: PaneOpenWriter,
) {
    let mut split_this_batch: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    let mut spawn_seq_overrides: std::collections::HashMap<Entity, u64> =
        std::collections::HashMap::new();
    let mut pending_leaf_infos: std::collections::HashMap<Entity, LeafInfo> =
        std::collections::HashMap::new();
    let mut pending_leaf_stacks: std::collections::HashMap<Entity, Vec<Entity>> =
        std::collections::HashMap::new();
    let mut pending_open_stacks: Vec<(String, Entity)> = Vec::new();
    let mut retired_leaf_panes: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for req in reader.read() {
        let placement = resolver.placements.resolve(&req.url);
        let reuse = resolver.space_hierarchy.get(req.pane).and_then(|space| {
            find_reuse_in_space(
                &req.url,
                space,
                &resolver.tab_q,
                &resolver.all_children,
                &resolver.page_q,
                &resolver.open_task_q,
                &child_of_q,
                &resolver.placements,
            )
        });
        if let Some(hit) = reuse {
            if let Ok(meta) = resolver.page_q.get(hit.stack)
                && meta.url != req.url
            {
                writer.open(hit.stack, req.url.clone(), None);
            }
            if req.focus {
                focus_reuse_hit(&mut tree.commands, &child_of_q, hit);
            }
            continue;
        }
        if req.direction.is_none()
            && let Some(index) =
                pending_open_match_index(&req.url, &pending_open_stacks, &resolver.placements)
        {
            let (pending_url, stack) = &mut pending_open_stacks[index];
            if *pending_url != req.url {
                writer.open(*stack, req.url.clone(), None);
                *pending_url = req.url.clone();
            }
            if req.focus {
                focus_stack_in_layout(&mut tree.commands, &child_of_q, &focus, *stack);
            }
            continue;
        }

        if let Some(direction) = req.direction {
            let (target_pane, pending_size, refresh_spawn_seq) =
                match pane_hierarchy.sibling(req.pane, direction) {
                    Some(sibling) => (sibling, pane_size(sibling, &resolver.node_q), false),
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
                            &resolver.seq_q,
                            &resolver.node_q,
                            &resolver.page_q,
                            &spawn_seq_overrides,
                            &resolver.placements,
                        );
                        let split_dir = PaneSplitDirection::from(direction);
                        let already_split =
                            !split_this_batch.insert(req.pane) || split_dir_q.contains(req.pane);
                        let split = split_or_extend_for_batch(
                            &mut tree,
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
                            &mut tree.commands,
                            &mut writer,
                            &mut spawn_seq_overrides,
                            &mut pending_leaf_infos,
                            split.holder,
                            split.target,
                        );
                        let pending_size = split
                            .target_size
                            .unwrap_or_else(|| pane_size(split.target, &resolver.node_q));
                        (split.target, pending_size, false)
                    }
                };
            let stack = spawn_beside_stack(
                &mut tree.commands,
                target_pane,
                req,
                placement,
                &resolver.placements,
                &mut writer,
                &mut spawn_seq_overrides,
                &mut pending_leaf_infos,
                &mut pending_leaf_stacks,
                pending_size,
                refresh_spawn_seq,
            );
            pending_open_stacks.push((req.url.clone(), stack));
            continue;
        }

        let Some(tab) = focus.tab_of(req.pane) else {
            let stack = spawn_beside_stack(
                &mut tree.commands,
                req.pane,
                req,
                placement,
                &resolver.placements,
                &mut writer,
                &mut spawn_seq_overrides,
                &mut pending_leaf_infos,
                &mut pending_leaf_stacks,
                pane_size(req.pane, &resolver.node_q),
                false,
            );
            pending_open_stacks.push((req.url.clone(), stack));
            continue;
        };
        let mut leaves = PanePlacement::collect_leaf_infos(
            tab,
            &resolver.all_children,
            &leaf_panes,
            &pane_children,
            &resolver.seq_q,
            &resolver.node_q,
            &resolver.page_q,
            &spawn_seq_overrides,
            &resolver.placements,
        );
        leaves.retain(|leaf| !retired_leaf_panes.contains(&leaf.pane));
        merge_pending_leaf_infos(&mut leaves, &pending_leaf_infos);

        match Placement::resolve(placement, reuse, &leaves, req.pane) {
            Placement::Focus { tab, stack } => {
                focus_reuse_hit(&mut tree.commands, &child_of_q, ReuseHit { tab, stack });
            }
            Placement::AddTab { pane } => {
                let stack = spawn_beside_stack(
                    &mut tree.commands,
                    pane,
                    req,
                    placement,
                    &resolver.placements,
                    &mut writer,
                    &mut spawn_seq_overrides,
                    &mut pending_leaf_infos,
                    &mut pending_leaf_stacks,
                    pane_size(pane, &resolver.node_q),
                    placement.refresh,
                );
                pending_open_stacks.push((req.url.clone(), stack));
            }
            Placement::Spiral { anchor, axis } => {
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
                    &mut tree,
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
                    &mut tree.commands,
                    &mut writer,
                    &mut spawn_seq_overrides,
                    &mut pending_leaf_infos,
                    split.holder,
                    split.target,
                );
                let pending_size = split
                    .target_size
                    .unwrap_or_else(|| pane_size(anchor, &resolver.node_q));
                let stack = spawn_beside_stack(
                    &mut tree.commands,
                    split.target,
                    req,
                    placement,
                    &resolver.placements,
                    &mut writer,
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
    tree: &mut PaneTree,
    anchor: Entity,
    split_dir: PaneSplitDirection,
    existing_tabs: &[Entity],
    activate_new: bool,
    already_split: bool,
    old_leaf_info: Option<LeafInfo>,
    pending_leaf_infos: &mut std::collections::HashMap<Entity, LeafInfo>,
    pending_leaf_stacks: &mut std::collections::HashMap<Entity, Vec<Entity>>,
    retired_leaf_panes: &mut std::collections::HashSet<Entity>,
) -> BatchSplit {
    if already_split {
        return BatchSplit {
            target: tree.split_or_extend(anchor, split_dir, existing_tabs, activate_new, true),
            holder: None,
            target_size: None,
        };
    }

    let pending_info = pending_leaf_infos.remove(&anchor);
    pending_leaf_stacks.remove(&anchor);
    let (holder, target) = tree.spawn_split(anchor, split_dir, existing_tabs, activate_new);
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
    writer: &mut PaneOpenWriter,
    spawn_seq_overrides: &mut std::collections::HashMap<Entity, u64>,
    pending_leaf_infos: &mut std::collections::HashMap<Entity, LeafInfo>,
    holder: Option<Entity>,
    target: Entity,
) {
    let mut stamp = |pane| {
        let seq = writer.touch(commands, pane);
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

fn focus_reuse_hit(commands: &mut Commands, child_of_q: &Query<&ChildOf>, hit: ReuseHit) {
    if let Ok(co) = child_of_q.get(hit.stack) {
        commands.entity(co.get()).insert(LastActivatedAt::now());
    }
    commands.entity(hit.stack).insert(LastActivatedAt::now());
    commands.entity(hit.tab).insert(LastActivatedAt::now());
}

fn focus_stack_in_layout(
    commands: &mut Commands,
    child_of_q: &Query<&ChildOf>,
    focus: &LayoutFocus,
    stack: Entity,
) {
    if let Ok(co) = child_of_q.get(stack) {
        let pane = co.get();
        commands.entity(pane).insert(LastActivatedAt::now());
        if let Some(tab) = focus.tab_of(pane) {
            commands.entity(tab).insert(LastActivatedAt::now());
        }
    }
    commands.entity(stack).insert(LastActivatedAt::now());
}

fn current_pane_spawn_seq(
    pane: Entity,
    seq_q: &Query<&SpawnSeq>,
    spawn_seq_overrides: &std::collections::HashMap<Entity, u64>,
    pending_leaf_infos: &std::collections::HashMap<Entity, LeafInfo>,
) -> u64 {
    pending_leaf_infos
        .get(&pane)
        .map(|info| info.spawn_seq)
        .or_else(|| spawn_seq_overrides.get(&pane).copied())
        .or_else(|| seq_q.get(pane).ok().map(|s| s.0))
        .unwrap_or(0)
}

fn spawn_beside_stack(
    commands: &mut Commands,
    target_pane: Entity,
    req: &OpenBesideRequest,
    placement: PagePlacement,
    placements: &PagePlacementCatalog,
    writer: &mut PaneOpenWriter,
    spawn_seq_overrides: &mut std::collections::HashMap<Entity, u64>,
    pending_leaf_infos: &mut std::collections::HashMap<Entity, LeafInfo>,
    pending_leaf_stacks: &mut std::collections::HashMap<Entity, Vec<Entity>>,
    pending_size: Vec2,
    refresh_spawn_seq: bool,
) -> Entity {
    let spawn_seq = if refresh_spawn_seq {
        let seq = writer.touch(commands, target_pane);
        spawn_seq_overrides.insert(target_pane, seq.0);
        seq.0
    } else {
        current_pane_spawn_seq(
            target_pane,
            &writer.sequences,
            spawn_seq_overrides,
            pending_leaf_infos,
        )
    };
    record_pending_leaf_info(
        pending_leaf_infos,
        target_pane,
        placement,
        spawn_seq,
        pending_size,
    );
    let stack_ts = if req.focus {
        LastActivatedAt::now()
    } else {
        LastActivatedAt(0)
    };
    let new_stack = commands
        .spawn((Stack::bundle(), stack_ts, ChildOf(target_pane)))
        .id();
    commands.entity(new_stack).insert(PageMetadata {
        url: req.url.clone(),
        ..default()
    });
    pending_leaf_stacks
        .entry(target_pane)
        .or_default()
        .push(new_stack);
    writer.open(
        new_stack,
        req.url.clone(),
        (!placements.registered(&req.url)).then_some(req.request_id),
    );
    new_stack
}

fn pending_open_match_index(
    url: &str,
    pending_open_stacks: &[(String, Entity)],
    placements: &PagePlacementCatalog,
) -> Option<usize> {
    pending_open_stacks
        .iter()
        .position(|(pending_url, _)| placements.reuses(url, pending_url))
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
    pending_leaf_infos: &mut std::collections::HashMap<Entity, LeafInfo>,
    pane: Entity,
    placement: PagePlacement,
    spawn_seq: u64,
    size: Vec2,
) {
    let info = pending_leaf_infos.entry(pane).or_insert_with(|| LeafInfo {
        pane,
        placements: Vec::new(),
        spawn_seq,
        size,
    });
    if !info
        .placements
        .iter()
        .any(|existing| existing.group == placement.group)
    {
        info.placements.push(placement);
    }
    info.spawn_seq = spawn_seq;
    if info.size == Vec2::ZERO {
        info.size = size;
    }
}

fn merge_pending_leaf_infos(
    leaves: &mut Vec<LeafInfo>,
    pending_leaf_infos: &std::collections::HashMap<Entity, LeafInfo>,
) {
    for pending in pending_leaf_infos.values() {
        if let Some(existing) = leaves.iter_mut().find(|leaf| leaf.pane == pending.pane) {
            for placement in &pending.placements {
                if !existing
                    .placements
                    .iter()
                    .any(|existing| existing.group == placement.group)
                {
                    existing.placements.push(*placement);
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
    page_q: &Query<&PageMetadata, With<Stack>>,
    spawn_seq_overrides: &std::collections::HashMap<Entity, u64>,
    placements: &PagePlacementCatalog,
) -> Option<LeafInfo> {
    let placements = unique_page_placements(
        pane_children
            .get(pane)
            .ok()?
            .iter()
            .filter_map(|child| page_q.get(child).ok())
            .map(|p| p.url.as_str()),
        placements,
    );
    Some(LeafInfo {
        pane,
        placements,
        spawn_seq: spawn_seq_overrides
            .get(&pane)
            .copied()
            .or_else(|| seq_q.get(pane).ok().map(|s| s.0))
            .unwrap_or(0),
        size: node_q.get(pane).map(|n| n.size).unwrap_or(Vec2::ZERO),
    })
}

fn unique_page_placements<'a>(
    urls: impl Iterator<Item = &'a str>,
    catalog: &PagePlacementCatalog,
) -> Vec<PagePlacement> {
    let mut placements = Vec::new();
    for url in urls {
        let placement = catalog.resolve(url);
        if !placements
            .iter()
            .any(|existing: &PagePlacement| existing.group == placement.group)
        {
            placements.push(placement);
        }
    }
    placements
}

fn find_reuse_in_space(
    url: &str,
    space: Entity,
    tab_q: &Query<Entity, With<Tab>>,
    all_children: &Query<&Children>,
    page_q: &Query<&PageMetadata, With<Stack>>,
    open_task_q: &Query<&PageOpenTask>,
    child_of_q: &Query<&ChildOf>,
    placements: &PagePlacementCatalog,
) -> Option<ReuseHit> {
    let tabs: Vec<Entity> = all_children
        .get(space)
        .map(|c| c.iter().filter(|&e| tab_q.contains(e)).collect())
        .unwrap_or_default();
    for tab in tabs {
        let mut frontier = vec![tab];
        while let Some(node) = frontier.pop() {
            if let Ok(meta) = page_q.get(node)
                && placements.reuses(url, &meta.url)
            {
                return Some(ReuseHit { tab, stack: node });
            }
            if let Ok(children) = all_children.get(node) {
                frontier.extend(children.iter());
            }
        }
    }
    for task in open_task_q.iter() {
        if !placements.reuses(url, &task.url) {
            continue;
        }
        if let Some(tab) = tab_for_stack_in_space(task.stack, space, child_of_q, tab_q) {
            return Some(ReuseHit {
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
pub struct PanePlacement<'w, 's> {
    pub child_of_q: Query<'w, 's, &'static ChildOf>,
    pub tab_q: Query<'w, 's, Entity, With<Tab>>,
    pub all_children: Query<'w, 's, &'static Children>,
    pub leaf_panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    pub pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    pub split_dir_q: Query<'w, 's, &'static PaneSplit>,
    pub tab_filter: Query<'w, 's, Entity, With<Stack>>,
    pub seq_q: Query<'w, 's, &'static SpawnSeq>,
    pub node_q: Query<'w, 's, &'static ComputedNode>,
    pub page_q: Query<'w, 's, &'static PageMetadata, With<Stack>>,
    pub placements: PagePlacementCatalog<'w, 's>,
    pub tree: PaneTree<'w, 's>,
}

impl PanePlacement<'_, '_> {
    fn collect_leaf_infos(
        tab: Entity,
        all_children: &Query<&Children>,
        leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
        pane_children: &Query<&Children, With<Pane>>,
        seq_q: &Query<&SpawnSeq>,
        node_q: &Query<&ComputedNode>,
        page_q: &Query<&PageMetadata, With<Stack>>,
        spawn_seq_overrides: &std::collections::HashMap<Entity, u64>,
        placements: &PagePlacementCatalog,
    ) -> Vec<LeafInfo> {
        let mut panes = Vec::new();
        let mut pending = vec![tab];
        while let Some(entity) = pending.pop() {
            if leaf_panes.contains(entity) {
                panes.push(entity);
            }
            if let Ok(children) = all_children.get(entity) {
                pending.extend(children.iter());
            }
        }
        panes
            .into_iter()
            .map(|pane| {
                let placements = pane_children
                    .get(pane)
                    .map(|children| {
                        unique_page_placements(
                            children
                                .iter()
                                .filter_map(|child| page_q.get(child).ok())
                                .map(|page| page.url.as_str()),
                            placements,
                        )
                    })
                    .unwrap_or_default();
                LeafInfo {
                    pane,
                    placements,
                    spawn_seq: spawn_seq_overrides
                        .get(&pane)
                        .copied()
                        .or_else(|| seq_q.get(pane).ok().map(|sequence| sequence.0))
                        .unwrap_or(0),
                    size: node_q.get(pane).map(|node| node.size).unwrap_or(Vec2::ZERO),
                }
            })
            .collect()
    }

    fn tab_of(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        for _ in 0..32 {
            if self.tab_q.contains(current) {
                return Some(current);
            }
            current = self.child_of_q.get(current).ok()?.parent();
        }
        None
    }

    pub fn resolve_spiral(
        &mut self,
        anchor_pane: Entity,
        url: &str,
        focus: bool,
        split_batch: &mut std::collections::HashSet<Entity>,
    ) -> Entity {
        let Some(tab) = self.tab_of(anchor_pane) else {
            return anchor_pane;
        };
        let leaves = Self::collect_leaf_infos(
            tab,
            &self.all_children,
            &self.leaf_panes,
            &self.pane_children,
            &self.seq_q,
            &self.node_q,
            &self.page_q,
            &std::collections::HashMap::new(),
            &self.placements,
        );
        match Placement::resolve(self.placements.resolve(url), None, &leaves, anchor_pane) {
            Placement::AddTab { pane } => pane,
            Placement::Spiral { anchor, axis } => {
                let existing_tabs: Vec<Entity> = self
                    .pane_children
                    .get(anchor)
                    .map(|children| {
                        children
                            .iter()
                            .filter(|&entity| self.tab_filter.contains(entity))
                            .collect()
                    })
                    .unwrap_or_default();
                let already_split =
                    !split_batch.insert(anchor) || self.split_dir_q.contains(anchor);
                self.tree
                    .split_or_extend(anchor, axis, &existing_tabs, focus, already_split)
            }
            Placement::Focus { .. } => anchor_pane,
        }
    }

    pub fn split_anchor(&self, anchor_pane: Entity) -> Entity {
        let Some(tab) = self.tab_of(anchor_pane) else {
            return anchor_pane;
        };
        let leaves = Self::collect_leaf_infos(
            tab,
            &self.all_children,
            &self.leaf_panes,
            &self.pane_children,
            &self.seq_q,
            &self.node_q,
            &self.page_q,
            &std::collections::HashMap::new(),
            &self.placements,
        );
        Placement::split_anchor(&leaves, anchor_pane)
    }
}

fn handle_in(
    mut reader: MessageReader<OpenRequest>,
    active_tab_param: ActiveTabParam,
    focus: LayoutFocus,
    pane_children: Query<&Children, With<Pane>>,
    tab_filter: Query<Entity, With<Stack>>,
    pane_stacks: PaneStacks,
    focused_space: crate::space::FocusedSpace,
    pane_hierarchy: PaneHierarchy,
    mut tree: PaneTree,
    mut commands: Commands,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
) {
    for request in reader.read() {
        let OpenRequest {
            direction,
            target,
            mode,
            url,
        } = request;

        let (_, active_pane_opt, _) = focus.resolve(active_tab_param.get());
        let Some(active) = active_pane_opt else {
            continue;
        };

        let resolved = url
            .clone()
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| focused_space.resolved_startup_url());

        let split_dir = PaneSplitDirection::from(*direction);

        let (target_pane, was_split) = match target {
            PaneTarget::Existing => match pane_hierarchy.sibling(active, *direction) {
                Some(sibling) => (sibling, false),
                None => {
                    let existing_tabs: Vec<Entity> = pane_children
                        .get(active)
                        .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                        .unwrap_or_default();
                    let p2 = tree.split_leaf(active, split_dir, &existing_tabs, true);
                    (p2, true)
                }
            },
            PaneTarget::NewSplit => {
                let existing_tabs: Vec<Entity> = pane_children
                    .get(active)
                    .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                    .unwrap_or_default();
                let p2 = tree.split_leaf(active, split_dir, &existing_tabs, true);
                (p2, true)
            }
        };

        if was_split {
            let new_stack = commands
                .spawn((
                    Stack::bundle(),
                    LastActivatedAt::now(),
                    ChildOf(target_pane),
                ))
                .id();
            page_open_requests.write(PageOpenRequest {
                target: PageOpenTarget::Stack(new_stack),
                url: resolved,
                request_id: None,
            });
        } else {
            match mode {
                PaneOpenMode::InPlace => {
                    let active_stack = focus
                        .stack(target_pane)
                        .or_else(|| pane_stacks.first(target_pane));
                    if let Some(stack) = active_stack {
                        page_open_requests.write(PageOpenRequest {
                            target: PageOpenTarget::Stack(stack),
                            url: resolved,
                            request_id: None,
                        });
                    }
                }
                PaneOpenMode::NewStack => {
                    let new_stack = commands
                        .spawn((
                            Stack::bundle(),
                            LastActivatedAt::now(),
                            ChildOf(target_pane),
                        ))
                        .id();
                    page_open_requests.write(PageOpenRequest {
                        target: PageOpenTarget::Stack(new_stack),
                        url: resolved,
                        request_id: None,
                    });
                }
            }
        }
        commands.entity(target_pane).insert(PendingCursorWarp);
    }
}
