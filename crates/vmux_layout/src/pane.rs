use crate::{
    CloseRequiresConfirmation, NewStackContext,
    settings::{ConfirmCloseSettings, LayoutSettings},
    stack::{
        CloseConfirmed, PendingStackClose, Stack, active_among, active_pane_in_tab,
        active_stack_in_pane, focused_stack, stack_bundle,
    },
    swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
    tab::Tab,
};
use bevy::{
    ecs::{
        lifecycle::HookContext, message::Messages, relationship::Relationship, world::DeferredWorld,
    },
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    ui::{FlexDirection, UiGlobalTransform},
    window::{ClosingWindow, PrimaryWindow},
};
use bevy_cef::prelude::CefKeyboardTarget;
use moonshine_save::prelude::*;
use std::time::Instant;
use vmux_command::{
    AppCommand, BrowserCommand, LayoutCommand, OpenCommand, PaneCommand, ReadAppCommands,
    open::{PaneDirection, PaneOpenMode, PaneTarget},
};
use vmux_core::{PageOpenRequest, PageOpenTarget};
use vmux_history::LastActivatedAt;

/// Marker: pane is waiting for close confirmation dialog.
#[derive(Component)]
pub struct PendingPaneClose;

pub struct PanePlugin;

const HOVER_DEBOUNCE_MS: u64 = 80;
const HOVER_COOLDOWN_MS: u64 = 300;

#[derive(Resource, Default)]
pub struct PaneHoverIntent {
    pub target: Option<Entity>,
    pub since: Option<Instant>,
    pub last_activation: Option<Instant>,
}

impl Plugin for PanePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Pane>()
            .register_type::<PaneSplit>()
            .register_type::<PaneSplitDirection>()
            .register_type::<PaneSize>()
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .add_systems(Update, on_pane_select.in_set(ReadAppCommands))
            .add_systems(Update, handle_pane_commands.in_set(ReadAppCommands))
            .add_systems(Update, handle_open_in_pane.in_set(ReadAppCommands))
            .add_systems(
                Update,
                handle_zoom_command
                    .in_set(ReadAppCommands)
                    .before(handle_pane_commands),
            )
            .add_systems(
                Update,
                (
                    poll_cursor_pane_focus,
                    click_pane_in_player_mode,
                    pane_gap_drag_resize,
                    process_pending_pane_closes,
                    process_pending_stack_closes,
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    sync_pane_split_gaps_to_settings,
                    sync_zoom_visibility.before(bevy::ui::UiSystems::Layout),
                    clear_zoom_on_pane_removal,
                    warp_cursor_to_active_pane,
                ),
            );
        register_zoom_hooks(app);
    }
}

fn register_zoom_hooks(app: &mut App) {
    app.world_mut()
        .register_component_hooks::<Zoomed>()
        .on_remove(|mut world: DeferredWorld, ctx: HookContext| {
            let Some(z) = world.get::<Zoomed>(ctx.entity) else {
                return;
            };
            let hidden = z.hidden.clone();
            for e in hidden {
                if let Some(mut node) = world.get_mut::<Node>(e) {
                    node.display = Display::Flex;
                }
            }
        });
}

fn clear_zoom_on_pane_removal(
    mut removed: RemovedComponents<Pane>,
    zoomed_q: Query<(Entity, &Zoomed)>,
    mut commands: Commands,
) {
    let removed_set: Vec<Entity> = removed.read().collect();
    if removed_set.is_empty() {
        return;
    }
    for (tab, z) in &zoomed_q {
        if removed_set.contains(&z.leaf) {
            commands.entity(tab).remove::<Zoomed>();
        }
    }
}

/// Signals that the cursor should be warped to the active pane once layout is computed.
#[derive(Resource, Default)]
pub struct PendingCursorWarp {
    pub target: Option<Entity>,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct Pane;

#[derive(Component, Debug)]
pub struct Zoomed {
    pub leaf: Entity,
    pub hidden: Vec<Entity>,
}

fn tab_of(
    leaf: Entity,
    child_of_q: &Query<&ChildOf>,
    tabs: &Query<(Entity, &LastActivatedAt), With<Tab>>,
) -> Option<Entity> {
    let mut cur = leaf;
    loop {
        if tabs.get(cur).is_ok() {
            return Some(cur);
        }
        cur = child_of_q.get(cur).ok()?.0;
    }
}

fn collect_siblings_to_hide(
    leaf: Entity,
    tab: Entity,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    split_dir_q: &Query<&PaneSplit>,
) -> Vec<Entity> {
    let mut result = Vec::new();
    let mut cur = leaf;
    while cur != tab {
        let Ok(parent) = child_of_q.get(cur).map(|p| p.0) else {
            break;
        };
        if split_dir_q.get(parent).is_ok()
            && let Ok(children) = all_children.get(parent)
        {
            for child in children.iter() {
                if child != cur {
                    result.push(child);
                }
            }
        }
        cur = parent;
    }
    result
}

fn handle_zoom_command(
    mut reader: MessageReader<AppCommand>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    child_of_q: Query<&ChildOf>,
    split_dir_q: Query<&PaneSplit>,
    zoomed_q: Query<(), With<Zoomed>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let unzoom_only = match cmd {
            AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft
                | PaneCommand::SelectRight
                | PaneCommand::SelectUp
                | PaneCommand::SelectDown,
            )) => true,
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InPane { .. })) => true,
            AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)) => false,
            _ => continue,
        };
        let (_, active_pane_opt, _) = focused_stack(
            &tabs,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let Some(active) = active_pane_opt else {
            continue;
        };
        let Some(tab) = tab_of(active, &child_of_q, &tabs) else {
            continue;
        };

        if unzoom_only {
            if zoomed_q.get(tab).is_ok() {
                commands.entity(tab).remove::<Zoomed>();
            }
            continue;
        }

        if zoomed_q.get(tab).is_ok() {
            commands.entity(tab).remove::<Zoomed>();
        } else {
            let hidden =
                collect_siblings_to_hide(active, tab, &child_of_q, &all_children, &split_dir_q);
            if !hidden.is_empty() {
                commands.entity(tab).insert(Zoomed {
                    leaf: active,
                    hidden,
                });
            }
        }
    }
}

fn sync_zoom_visibility(zoomed_q: Query<&Zoomed, Added<Zoomed>>, mut nodes: Query<&mut Node>) {
    for z in &zoomed_q {
        for &e in &z.hidden {
            if let Ok(mut node) = nodes.get_mut(e) {
                node.display = Display::None;
            }
        }
    }
}

#[cfg(test)]
fn siblings_to_hide(world: &World, leaf: Entity, tab: Entity) -> Vec<Entity> {
    let mut result = Vec::new();
    let mut cur = leaf;
    while cur != tab {
        let Some(parent) = world.get::<ChildOf>(cur).map(|p| p.0) else {
            break;
        };
        if world.get::<PaneSplit>(parent).is_some()
            && let Some(children) = world.get::<Children>(parent)
        {
            for child in children.iter() {
                if child != cur {
                    result.push(child);
                }
            }
        }
        cur = parent;
    }
    result
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

#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct PaneSize {
    pub flex_grow: f32,
}

impl Default for PaneSize {
    fn default() -> Self {
        Self { flex_grow: 1.0 }
    }
}

pub const MIN_PANE_PX: f32 = 60.0;
pub const RESIZE_STEP: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneSplitGaps {
    pub column_gap: Val,
    pub row_gap: Val,
}

pub fn pane_split_gaps(direction: PaneSplitDirection, gap: f32) -> PaneSplitGaps {
    match direction {
        PaneSplitDirection::Row => PaneSplitGaps {
            column_gap: Val::Px(gap),
            row_gap: Val::Px(0.0),
        },
        PaneSplitDirection::Column => PaneSplitGaps {
            column_gap: Val::Px(0.0),
            row_gap: Val::Px(gap),
        },
    }
}

pub fn apply_pane_split_gaps(split: &PaneSplit, node: &mut Node, gap: f32) {
    let gaps = pane_split_gaps(split.direction, gap);
    node.column_gap = gaps.column_gap;
    node.row_gap = gaps.row_gap;
}

/// Temporary component inserted on a PaneSplit entity while the user is
/// dragging the gap between two of its children.
#[derive(Component)]
pub struct PaneDrag {
    prev_child: Entity,
    next_child: Entity,
    start_pos: f32,
    start_prev_grow: f32,
    start_next_grow: f32,
}

pub fn leaf_pane_bundle() -> impl Bundle {
    (
        Pane,
        PaneSize::default(),
        Transform::default(),
        GlobalTransform::default(),
        Node {
            flex_grow: 1.0,
            flex_basis: Val::Px(0.0),
            align_items: AlignItems::Stretch,
            justify_content: JustifyContent::Stretch,
            ..default()
        },
    )
}

pub fn split_root_bundle(direction: PaneSplitDirection) -> impl Bundle {
    let flex_direction = match direction {
        PaneSplitDirection::Row => FlexDirection::Row,
        PaneSplitDirection::Column => FlexDirection::Column,
    };
    let gap = pane_split_gaps(direction, crate::event::PANE_GAP_PX);
    (
        Pane,
        PaneSplit { direction },
        PaneSize::default(),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::default(),
        Node {
            flex_grow: 1.0,
            flex_direction,
            column_gap: gap.column_gap,
            row_gap: gap.row_gap,
            align_items: AlignItems::Stretch,
            ..default()
        },
    )
}

/// Compute clamped flex_grow values after a resize delta.
/// Returns (new_pane_grow, new_sibling_grow).
fn compute_resize(pane_grow: f32, sib_grow: f32, delta: f32, parent_len: f32) -> (f32, f32) {
    let total = pane_grow + sib_grow;
    let mut pg = pane_grow + delta;
    let mut sg = sib_grow - delta;

    let min_grow = MIN_PANE_PX / parent_len.max(1.0) * total;
    pg = pg.max(min_grow);
    sg = sg.max(min_grow);

    let new_total = pg + sg;
    if new_total > 0.0 {
        pg = pg / new_total * total;
        sg = sg / new_total * total;
    }
    (pg, sg)
}

pub fn first_leaf_descendant(
    entity: Entity,
    children_q: &Query<&Children, With<Pane>>,
    leaf_q: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Entity {
    if leaf_q.contains(entity) {
        return entity;
    }
    if let Ok(children) = children_q.get(entity) {
        for child in children.iter() {
            if leaf_q.contains(child) {
                return child;
            }
            let found = first_leaf_descendant(child, children_q, leaf_q);
            if found != child || leaf_q.contains(found) {
                return found;
            }
        }
    }
    entity
}

pub fn first_stack_in_pane(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    tab_q: &Query<Entity, With<Stack>>,
) -> Option<Entity> {
    let children = pane_children.get(pane).ok()?;
    children.iter().find(|&e| tab_q.contains(e))
}

#[derive(bevy::ecs::system::SystemParam)]
struct PaneStartupContext<'w> {
    effective: Option<Res<'w, crate::settings::EffectiveStartupUrl>>,
    requests: MessageWriter<'w, PageOpenRequest>,
    new_stack_ctx: ResMut<'w, NewStackContext>,
}

impl PaneStartupContext<'_> {
    fn url(&self) -> String {
        self.effective
            .as_deref()
            .map(|u| u.0.clone())
            .unwrap_or_default()
    }
}

fn handle_pane_commands(
    mut reader: MessageReader<AppCommand>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    child_of_q: Query<&ChildOf>,
    split_dir_q: Query<&PaneSplit>,
    tab_filter: Query<Entity, With<Stack>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
    mut startup: PaneStartupContext,
    mut resize_q: ParamSet<(
        Query<&mut Node>,
        Query<&mut PaneSize>,
        Query<&ComputedNode>,
        ResMut<PendingCursorWarp>,
        Query<'static, 'static, (), With<CloseRequiresConfirmation>>,
        Query<'static, 'static, (), With<CloseConfirmed>>,
        Query<'static, 'static, (), With<PendingPaneClose>>,
        Res<'static, ConfirmCloseSettings>,
    )>,
) {
    for cmd in reader.read() {
        let AppCommand::Layout(LayoutCommand::Pane(pane_cmd)) = *cmd else {
            continue;
        };
        let (_, active_pane_opt, _active_stack_opt) = focused_stack(
            &tabs,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let Some(active) = active_pane_opt else {
            continue;
        };

        match pane_cmd {
            PaneCommand::Close => {
                let confirm_enabled = resize_q.p7().enabled;
                let needs_confirm = confirm_enabled
                    && pane_has_close_confirmation(
                        active,
                        &pane_children,
                        &all_children,
                        &resize_q.p4(),
                    );
                if needs_confirm {
                    if resize_q.p5().contains(active) {
                        commands.entity(active).remove::<CloseConfirmed>();
                    } else {
                        if !resize_q.p6().contains(active) {
                            commands.entity(active).insert(PendingPaneClose);
                        }
                        continue;
                    }
                }

                let Ok(pane_co) = child_of_q.get(active) else {
                    continue;
                };
                let parent = pane_co.get();

                if !split_dir_q.contains(parent) {
                    if leaf_panes.iter().count() <= 1 {
                        commands.entity(*primary_window).insert(ClosingWindow);
                    } else {
                        commands.entity(active).despawn();
                        let leaf = commands
                            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(parent)))
                            .id();
                        let tab = commands
                            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(leaf)))
                            .id();
                        commands.entity(leaf).insert(LastActivatedAt::now());
                        let url = startup.url();
                        if url.is_empty() {
                            startup.new_stack_ctx.stack = Some(tab);
                            startup.new_stack_ctx.previous_stack = None;
                            startup.new_stack_ctx.needs_open = true;
                        } else {
                            startup.requests.write(PageOpenRequest {
                                target: PageOpenTarget::Stack(tab),
                                url,
                                request_id: None,
                            });
                        }
                    }
                    continue;
                }

                let Ok(siblings) = pane_children.get(parent) else {
                    continue;
                };
                let sibling = siblings
                    .iter()
                    .find(|&e| e != active && (leaf_panes.contains(e) || split_dir_q.contains(e)));
                let Some(sibling) = sibling else {
                    continue;
                };

                let sibling_children: Vec<Entity> = pane_children
                    .get(sibling)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();

                for &child in &sibling_children {
                    commands.entity(child).insert(ChildOf(parent));
                }

                let new_active_pane;
                if split_dir_q.contains(sibling) {
                    new_active_pane = first_leaf_descendant(sibling, &pane_children, &leaf_panes);
                    commands.entity(sibling).remove::<ChildOf>();
                    commands.queue(move |world: &mut World| {
                        world.despawn(sibling);
                    });
                } else {
                    new_active_pane = parent;
                    commands.entity(parent).remove::<PaneSplit>();
                    commands.entity(parent).insert(Node {
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        align_items: AlignItems::Stretch,
                        justify_content: JustifyContent::Stretch,
                        ..default()
                    });
                    commands.entity(sibling).despawn();
                }

                commands.entity(active).despawn();
                commands
                    .entity(new_active_pane)
                    .insert(LastActivatedAt::now());
                let tab = active_stack_in_pane(new_active_pane, &pane_children, &stack_ts)
                    .or_else(|| first_stack_in_pane(new_active_pane, &pane_children, &tab_filter))
                    .or_else(|| {
                        sibling_children
                            .iter()
                            .copied()
                            .find(|&e| tab_filter.contains(e))
                    });
                if let Some(tab) = tab {
                    commands.entity(tab).insert(LastActivatedAt::now());
                }
            }
            PaneCommand::Toggle => {}
            PaneCommand::Zoom => {}
            PaneCommand::SelectLeft => {}
            PaneCommand::SelectRight => {}
            PaneCommand::SelectUp => {}
            PaneCommand::SelectDown => {}
            PaneCommand::SwapPrev | PaneCommand::SwapNext => {
                let Ok(co) = child_of_q.get(active) else {
                    continue;
                };
                let parent = co.get();
                if !split_dir_q.contains(parent) {
                    continue;
                }
                let Ok(children) = all_children.get(parent) else {
                    continue;
                };
                let kind_positions: Vec<usize> = children
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| leaf_panes.contains(*e) || split_dir_q.contains(*e))
                    .map(|(i, _)| i)
                    .collect();
                let Some(active_idx) = find_kind_index(active, children, &kind_positions) else {
                    continue;
                };
                let pair = if pane_cmd == PaneCommand::SwapPrev {
                    resolve_prev(active_idx)
                } else {
                    resolve_next(active_idx, kind_positions.len())
                };
                if let Some((a, b)) = pair {
                    swap_siblings(&mut commands, parent, children, &kind_positions, a, b);
                }
            }
            PaneCommand::RotateForward => {}
            PaneCommand::RotateBackward => {}
            PaneCommand::EqualizeSize => {
                let Ok(co) = child_of_q.get(active) else {
                    continue;
                };
                let parent = co.get();
                if !split_dir_q.contains(parent) {
                    continue;
                }
                let Ok(children) = all_children.get(parent) else {
                    continue;
                };
                let targets: Vec<Entity> = children.iter().collect();
                {
                    let mut nq = resize_q.p0();
                    for &child in &targets {
                        if let Ok(mut node) = nq.get_mut(child) {
                            node.flex_grow = 1.0;
                        }
                    }
                }
                {
                    let mut sq = resize_q.p1();
                    for &child in &targets {
                        if let Ok(mut ps) = sq.get_mut(child) {
                            ps.flex_grow = 1.0;
                        }
                    }
                }
            }
            PaneCommand::ResizeLeft
            | PaneCommand::ResizeRight
            | PaneCommand::ResizeUp
            | PaneCommand::ResizeDown => {
                let target_axis = match pane_cmd {
                    PaneCommand::ResizeLeft | PaneCommand::ResizeRight => PaneSplitDirection::Row,
                    _ => PaneSplitDirection::Column,
                };
                let grows = matches!(pane_cmd, PaneCommand::ResizeRight | PaneCommand::ResizeDown);

                let mut child_in_split = active;
                let mut found_parent: Option<Entity> = None;
                for _ in 0..10 {
                    let Ok(co) = child_of_q.get(child_in_split) else {
                        break;
                    };
                    let parent = co.get();
                    if let Ok(ps) = split_dir_q.get(parent)
                        && ps.direction == target_axis
                    {
                        found_parent = Some(parent);
                        break;
                    }
                    child_in_split = parent;
                }
                let Some(parent) = found_parent else { continue };
                let Ok(siblings) = all_children.get(parent) else {
                    continue;
                };
                let sibs: Vec<Entity> = siblings.iter().collect();
                let Some(idx) = sibs.iter().position(|&e| e == child_in_split) else {
                    continue;
                };

                let (pane_entity, sibling_entity) = if grows {
                    if idx + 1 >= sibs.len() {
                        continue;
                    }
                    (child_in_split, sibs[idx + 1])
                } else {
                    if idx == 0 {
                        continue;
                    }
                    (child_in_split, sibs[idx - 1])
                };

                // Read current values
                let parent_len;
                let pane_grow;
                let sib_grow;
                {
                    let cnq = resize_q.p2();
                    let ps = cnq.get(parent).map(|cn| cn.size).unwrap_or(Vec2::ZERO);
                    parent_len = match target_axis {
                        PaneSplitDirection::Row => ps.x,
                        PaneSplitDirection::Column => ps.y,
                    };
                }
                {
                    let nq = resize_q.p0();
                    pane_grow = nq.get(pane_entity).map_or(1.0, |n| n.flex_grow);
                    sib_grow = nq.get(sibling_entity).map_or(1.0, |n| n.flex_grow);
                }

                let total_grow = pane_grow + sib_grow;
                let step = RESIZE_STEP * total_grow;
                let (pg, sg) = compute_resize(pane_grow, sib_grow, step, parent_len);

                {
                    let mut nq = resize_q.p0();
                    if let Ok(mut n) = nq.get_mut(pane_entity) {
                        n.flex_grow = pg;
                    }
                    if let Ok(mut n) = nq.get_mut(sibling_entity) {
                        n.flex_grow = sg;
                    }
                }
                {
                    let mut sq = resize_q.p1();
                    if let Ok(mut ps) = sq.get_mut(pane_entity) {
                        ps.flex_grow = pg;
                    }
                    if let Ok(mut ps) = sq.get_mut(sibling_entity) {
                        ps.flex_grow = sg;
                    }
                }
            }
        }
    }
}

fn direction_to_split(direction: &PaneDirection) -> PaneSplitDirection {
    match direction {
        PaneDirection::Left | PaneDirection::Right => PaneSplitDirection::Row,
        PaneDirection::Top | PaneDirection::Bottom => PaneSplitDirection::Column,
    }
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
    mut reader: MessageReader<AppCommand>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    child_of_q: Query<&ChildOf>,
    split_dir_q: Query<&PaneSplit>,
    tab_filter: Query<Entity, With<Stack>>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut commands: Commands,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut pending_warp: ResMut<PendingCursorWarp>,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(BrowserCommand::Open(OpenCommand::InPane {
            direction,
            target,
            mode,
            url,
        })) = cmd
        else {
            continue;
        };

        let (_, active_pane_opt, _) = focused_stack(
            &tabs,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let Some(active) = active_pane_opt else {
            continue;
        };

        let resolved = vmux_command::open::handler::resolve_url(
            url.as_deref(),
            effective_startup_url.as_ref().map(|s| s.0.as_str()),
        );

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
                        let pane1 = commands
                            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(active)))
                            .id();
                        let p2 = commands
                            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(active)))
                            .id();
                        for tab in &existing_tabs {
                            commands.entity(*tab).insert(ChildOf(pane1));
                        }
                        commands.entity(active).insert(split_root_bundle(split_dir));
                        commands.entity(p2).insert(LastActivatedAt::now());
                        (p2, true)
                    }
                }
            }
            PaneTarget::NewSplit => {
                let existing_tabs: Vec<Entity> = pane_children
                    .get(active)
                    .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                    .unwrap_or_default();
                let pane1 = commands
                    .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(active)))
                    .id();
                let p2 = commands
                    .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(active)))
                    .id();
                for tab in &existing_tabs {
                    commands.entity(*tab).insert(ChildOf(pane1));
                }
                commands.entity(active).insert(split_root_bundle(split_dir));
                commands.entity(p2).insert(LastActivatedAt::now());
                (p2, true)
            }
        };

        if was_split {
            let new_stack = commands
                .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(target_pane)))
                .id();
            page_open_requests.write(PageOpenRequest {
                target: PageOpenTarget::Stack(new_stack),
                url: resolved,
                request_id: None,
            });
            pending_warp.target = Some(target_pane);
        } else {
            match mode {
                PaneOpenMode::InPlace => {
                    let active_stack = active_stack_in_pane(target_pane, &pane_children, &stack_ts)
                        .or_else(|| first_stack_in_pane(target_pane, &pane_children, &tab_filter));
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
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(target_pane)))
                        .id();
                    page_open_requests.write(PageOpenRequest {
                        target: PageOpenTarget::Stack(new_stack),
                        url: resolved,
                        request_id: None,
                    });
                }
            }
        }
    }
}

fn on_pane_select(
    mut reader: MessageReader<AppCommand>,
    tab_q: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_pos_q: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    mut pending_warp: ResMut<PendingCursorWarp>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let direction: Vec2 = match cmd {
            AppCommand::Layout(LayoutCommand::Pane(PaneCommand::SelectLeft)) => {
                Vec2::new(-1.0, 0.0)
            }
            AppCommand::Layout(LayoutCommand::Pane(PaneCommand::SelectRight)) => {
                Vec2::new(1.0, 0.0)
            }
            AppCommand::Layout(LayoutCommand::Pane(PaneCommand::SelectUp)) => Vec2::new(0.0, -1.0),
            AppCommand::Layout(LayoutCommand::Pane(PaneCommand::SelectDown)) => Vec2::new(0.0, 1.0),
            _ => continue,
        };

        if let Some(e) = new_stack_ctx.stack.take() {
            commands.entity(e).despawn();
            new_stack_ctx.previous_stack = None;
        }

        let active_tab = active_among(tab_q.iter());
        let Some(tab_e) = active_tab else {
            continue;
        };
        let panes = collect_tab_leaf_panes(tab_e, &all_children, &leaf_pane_q);
        if panes.len() < 2 {
            continue;
        }
        let current = active_pane_in_tab(tab_e, &all_children, &leaf_pane_q, &pane_ts);
        let Some(current) = current else {
            continue;
        };
        let Ok((cur_node, cur_gt)) = pane_pos_q.get(current) else {
            continue;
        };
        let cur_center = cur_gt.transform_point2(Vec2::ZERO);
        let cur_size = cur_node.size;

        let mut candidates: Vec<Entity> = Vec::new();
        for &pane in &panes {
            if pane == current {
                continue;
            }
            let Ok((tgt_node, gt)) = pane_pos_q.get(pane) else {
                continue;
            };
            let center = gt.transform_point2(Vec2::ZERO);
            let tgt_size = tgt_node.size;
            let delta = center - cur_center;

            let along = delta.dot(direction);
            if along <= 0.0 {
                continue;
            }

            let overlaps = if direction.x.abs() > 0.5 {
                let cur_min = cur_center.y - cur_size.y * 0.5;
                let cur_max = cur_center.y + cur_size.y * 0.5;
                let tgt_min = center.y - tgt_size.y * 0.5;
                let tgt_max = center.y + tgt_size.y * 0.5;
                cur_min.max(tgt_min) < cur_max.min(tgt_max)
            } else {
                let cur_min = cur_center.x - cur_size.x * 0.5;
                let cur_max = cur_center.x + cur_size.x * 0.5;
                let tgt_min = center.x - tgt_size.x * 0.5;
                let tgt_max = center.x + tgt_size.x * 0.5;
                cur_min.max(tgt_min) < cur_max.min(tgt_max)
            };
            if !overlaps {
                continue;
            }

            candidates.push(pane);
        }
        let best = active_among(candidates.iter().filter_map(|&e| pane_ts.get(e).ok()))
            .map(|e| (e, 0.0_f32));

        if let Some((target, _)) = best {
            hover_intent.target = None;
            hover_intent.last_activation = Some(Instant::now());
            commands.entity(target).insert(LastActivatedAt::now());
            pending_warp.target = Some(target);
        }
    }
}

fn poll_cursor_pane_focus(
    mode: Res<crate::scene::InteractionMode>,
    windows: Query<&Window, With<PrimaryWindow>>,
    leaf_panes: Query<
        (Entity, &ComputedNode, &UiGlobalTransform),
        (With<Pane>, Without<PaneSplit>),
    >,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    mut intent: ResMut<PaneHoverIntent>,
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    active_drags: Query<(), With<PaneDrag>>,
) {
    if *mode != crate::scene::InteractionMode::User {
        return;
    }
    if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
        return;
    }
    if !active_drags.is_empty() {
        return;
    }
    if let Some(last) = intent.last_activation
        && last.elapsed().as_millis() < HOVER_COOLDOWN_MS as u128
    {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.physical_cursor_position() else {
        return;
    };
    let cursor = Vec2::new(cursor_pos.x, cursor_pos.y);

    let mut hovered_pane: Option<Entity> = None;
    for (entity, node, ui_gt) in &leaf_panes {
        let center = ui_gt.transform_point2(Vec2::ZERO);
        let half = node.size * 0.5;
        let min = center - half;
        let max = center + half;
        if cursor.x >= min.x && cursor.x <= max.x && cursor.y >= min.y && cursor.y <= max.y {
            hovered_pane = Some(entity);
            break;
        }
    }

    let Some(target) = hovered_pane else {
        intent.target = None;
        return;
    };

    // Check if already the active pane
    let current_active = active_among(
        leaf_panes
            .iter()
            .filter_map(|(e, _, _)| pane_ts.get(e).ok()),
    );
    if current_active == Some(target) {
        intent.target = None;
        return;
    }

    if intent.target != Some(target) {
        intent.target = Some(target);
        intent.since = Some(Instant::now());
        return;
    }

    let Some(since) = intent.since else {
        return;
    };
    if since.elapsed().as_millis() < HOVER_DEBOUNCE_MS as u128 {
        return;
    }

    commands.entity(target).insert(LastActivatedAt::now());
    intent.target = None;
    intent.last_activation = Some(Instant::now());
}

fn click_pane_in_player_mode(
    mode: Res<crate::scene::InteractionMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    leaf_panes: Query<
        (Entity, &ComputedNode, &UiGlobalTransform),
        (With<Pane>, Without<PaneSplit>),
    >,
    kb_targets: Query<Entity, With<CefKeyboardTarget>>,
    mut commands: Commands,
    accumulated_motion: Res<AccumulatedMouseMotion>,
    mut press_motion: Local<Option<f32>>,
    mut last_click: Local<Option<(Entity, Instant)>>,
    transition: Option<Res<crate::scene::ModeTransition>>,
    mut camera_state: Single<
        &mut bevy::camera_controller::free_camera::FreeCameraState,
        With<crate::scene::MainCamera>,
    >,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
) {
    if *mode != crate::scene::InteractionMode::Player {
        *press_motion = None;
        *last_click = None;
        return;
    }

    // Don't handle clicks during transition
    if transition.is_some() {
        *press_motion = None;
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Some(cursor_pos) = window.physical_cursor_position() else {
        return;
    };
    let cursor = Vec2::new(cursor_pos.x, cursor_pos.y);

    if mouse.just_pressed(MouseButton::Left) {
        *press_motion = Some(0.0);
        return;
    }

    if let Some(ref mut total) = *press_motion {
        *total += accumulated_motion.delta.length();
    }

    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    let Some(total_motion) = press_motion.take() else {
        return;
    };
    const DRAG_THRESHOLD: f32 = 2.0;
    if total_motion > DRAG_THRESHOLD {
        return;
    }

    // Hit-test panes
    let mut hit_pane: Option<Entity> = None;
    for (entity, node, ui_gt) in &leaf_panes {
        let center = ui_gt.transform_point2(Vec2::ZERO);
        let half = node.size * 0.5;
        if cursor.x >= center.x - half.x
            && cursor.x <= center.x + half.x
            && cursor.y >= center.y - half.y
            && cursor.y <= center.y + half.y
        {
            hit_pane = Some(entity);
            break;
        }
    }

    if let Some(pane) = hit_pane {
        // Check for double-click
        const DOUBLE_CLICK_MS: u128 = 400;
        if let Some((prev_entity, prev_time)) = *last_click
            && prev_entity == pane
            && prev_time.elapsed().as_millis() < DOUBLE_CLICK_MS
        {
            // Double-click: exit player mode with animation
            *last_click = None;
            camera_state.enabled = false;
            suppress.0 = false;
            commands.insert_resource(crate::scene::ModeTransition::new(
                crate::scene::TransitionDirection::ExitPlayer,
            ));
            return;
        }

        // Single click: activate pane for keyboard input
        *last_click = Some((pane, Instant::now()));
        commands.entity(pane).insert(LastActivatedAt::now());
        // sync_keyboard_target in browser.rs will assign CefKeyboardTarget
        // to the active pane's browser, and suppress_free_camera_when_pane_active
        // will disable FreeCameraState when it detects the target.
    } else {
        // Clicked empty space: remove all keyboard targets (return to roaming)
        *last_click = None;
        for e in &kb_targets {
            commands.entity(e).remove::<CefKeyboardTarget>();
        }
    }
}

fn warp_cursor_to_active_pane(
    mut pending: ResMut<PendingCursorWarp>,
    pane_ui_q: Query<(&ComputedNode, &UiGlobalTransform), (With<Pane>, Without<PaneSplit>)>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Some(target) = pending.target else {
        return;
    };
    let Ok((node, ui_gt)) = pane_ui_q.get(target) else {
        return;
    };
    if node.size.x <= 0.0 || node.size.y <= 0.0 {
        return;
    }
    pending.target = None;
    let center = ui_gt.transform_point2(Vec2::ZERO);
    if let Ok(mut window) = windows.single_mut() {
        window.set_physical_cursor_position(Some(center.as_dvec2()));
    }
}

fn pane_gap_drag_resize(
    windows: Query<&Window, With<PrimaryWindow>>,

    splits: Query<(Entity, &PaneSplit, &Children), Without<PaneDrag>>,
    active_drags: Query<(Entity, &PaneDrag, &PaneSplit)>,
    child_nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    parent_nodes: Query<&ComputedNode>,
    mut node_q: Query<&mut Node>,
    mut size_q: Query<&mut PaneSize>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut commands: Commands,
) {
    let Ok(window) = windows.single() else { return };
    let Some(cursor_pos) = window.physical_cursor_position() else {
        return;
    };
    let cursor = Vec2::new(cursor_pos.x, cursor_pos.y);

    // --- Handle active drag ---
    if let Ok((split_entity, drag, split)) = active_drags.single() {
        if mouse.pressed(MouseButton::Left) {
            let pos_along = match split.direction {
                PaneSplitDirection::Row => cursor.x,
                PaneSplitDirection::Column => cursor.y,
            };
            let parent_size = parent_nodes
                .get(split_entity)
                .map(|cn| cn.size)
                .unwrap_or(Vec2::ONE);
            let parent_len = match split.direction {
                PaneSplitDirection::Row => parent_size.x,
                PaneSplitDirection::Column => parent_size.y,
            }
            .max(1.0);

            let (pg, sg) = compute_resize(
                drag.start_prev_grow,
                drag.start_next_grow,
                (pos_along - drag.start_pos) / parent_len
                    * (drag.start_prev_grow + drag.start_next_grow),
                parent_len,
            );

            if let Ok(mut n) = node_q.get_mut(drag.prev_child) {
                n.flex_grow = pg;
            }
            if let Ok(mut n) = node_q.get_mut(drag.next_child) {
                n.flex_grow = sg;
            }
            if let Ok(mut s) = size_q.get_mut(drag.prev_child) {
                s.flex_grow = pg;
            }
            if let Ok(mut s) = size_q.get_mut(drag.next_child) {
                s.flex_grow = sg;
            }
        } else {
            commands.entity(split_entity).remove::<PaneDrag>();
        }

        return;
    }

    // --- Hover detection + drag initiation ---
    'outer: for (split_entity, split, children) in &splits {
        let sibs: Vec<Entity> = children.iter().collect();
        for i in 0..sibs.len().saturating_sub(1) {
            let Ok((node_a, gt_a)) = child_nodes.get(sibs[i]) else {
                continue;
            };
            let Ok((node_b, gt_b)) = child_nodes.get(sibs[i + 1]) else {
                continue;
            };

            let center_a = gt_a.transform_point2(Vec2::ZERO);
            let center_b = gt_b.transform_point2(Vec2::ZERO);
            let half_a = node_a.size * 0.5;
            let half_b = node_b.size * 0.5;

            let (gap_min, gap_max, cross_min, cross_max) = match split.direction {
                PaneSplitDirection::Row => (
                    center_a.x + half_a.x,
                    center_b.x - half_b.x,
                    (center_a.y - half_a.y).min(center_b.y - half_b.y),
                    (center_a.y + half_a.y).max(center_b.y + half_b.y),
                ),
                PaneSplitDirection::Column => (
                    center_a.y + half_a.y,
                    center_b.y - half_b.y,
                    (center_a.x - half_a.x).min(center_b.x - half_b.x),
                    (center_a.x + half_a.x).max(center_b.x + half_b.x),
                ),
            };

            let (pos_along, pos_cross) = match split.direction {
                PaneSplitDirection::Row => (cursor.x, cursor.y),
                PaneSplitDirection::Column => (cursor.y, cursor.x),
            };

            if pos_along >= gap_min
                && pos_along <= gap_max
                && pos_cross >= cross_min
                && pos_cross <= cross_max
            {
                if mouse.just_pressed(MouseButton::Left) {
                    let prev_grow = node_q.get(sibs[i]).map(|n| n.flex_grow).unwrap_or(1.0);
                    let next_grow = node_q.get(sibs[i + 1]).map(|n| n.flex_grow).unwrap_or(1.0);
                    commands.entity(split_entity).insert(PaneDrag {
                        prev_child: sibs[i],
                        next_child: sibs[i + 1],
                        start_pos: pos_along,
                        start_prev_grow: prev_grow,
                        start_next_grow: next_grow,
                    });
                }
                break 'outer;
            }
        }
    }
}

fn sync_pane_split_gaps_to_settings(
    settings: Res<LayoutSettings>,
    mut splits: Query<(&PaneSplit, &mut Node), With<Pane>>,
) {
    if !settings.is_changed() {
        return;
    }
    for (split, mut node) in &mut splits {
        apply_pane_split_gaps(split, &mut node, crate::event::PANE_GAP_PX);
    }
}

fn collect_tab_leaf_panes(
    root: Entity,
    all_children: &Query<&Children>,
    leaf_q: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Vec<Entity> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if leaf_q.contains(entity) {
            result.push(entity);
        }
        if let Ok(children) = all_children.get(entity) {
            for child in children.iter() {
                stack.push(child);
            }
        }
    }
    result
}

fn pane_has_close_confirmation(
    pane: Entity,
    pane_children_q: &Query<&Children, With<Pane>>,
    all_children_q: &Query<&Children>,
    close_q: &Query<(), With<CloseRequiresConfirmation>>,
) -> bool {
    pane_children_q.get(pane).is_ok_and(|tabs| {
        tabs.iter()
            .any(|tab| entity_tree_has_close_confirmation(tab, all_children_q, close_q))
    })
}

fn entity_tree_has_close_confirmation(
    entity: Entity,
    all_children_q: &Query<&Children>,
    close_q: &Query<(), With<CloseRequiresConfirmation>>,
) -> bool {
    close_q.contains(entity)
        || all_children_q.get(entity).is_ok_and(|children| {
            children
                .iter()
                .any(|child| entity_tree_has_close_confirmation(child, all_children_q, close_q))
        })
}

fn show_close_dialog() -> bool {
    let result = rfd::MessageDialog::new()
        .set_title("Close terminal?")
        .set_description("A process is still running in this terminal. Close anyway?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();

    matches!(result, rfd::MessageDialogResult::Yes)
}

/// Exclusive system: processes pending pane close confirmations by showing
/// native dialogs on the main thread.
fn process_pending_pane_closes(world: &mut World) {
    let pending: Vec<Entity> = world
        .query_filtered::<Entity, (With<PendingPaneClose>, With<Pane>)>()
        .iter(world)
        .collect();

    if pending.is_empty() {
        return;
    }

    for pane in pending {
        let confirmed = show_close_dialog();

        if let Ok(mut entity_mut) = world.get_entity_mut(pane) {
            entity_mut.remove::<PendingPaneClose>();
        }

        if confirmed {
            if let Ok(mut entity_mut) = world.get_entity_mut(pane) {
                entity_mut.insert((CloseConfirmed, LastActivatedAt::now()));
            }
            let mut current = pane;
            for _ in 0..10 {
                if world.get_entity(current).is_ok_and(|e| e.contains::<Tab>()) {
                    if let Ok(mut entity_mut) = world.get_entity_mut(current) {
                        entity_mut.insert(LastActivatedAt::now());
                    }
                    break;
                }
                if let Some(co) = world.get::<ChildOf>(current) {
                    current = co.get();
                } else {
                    break;
                }
            }
            world
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close)));
        }
    }
}

fn process_pending_stack_closes(world: &mut World) {
    let pending: Vec<Entity> = world
        .query_filtered::<Entity, (With<PendingStackClose>, With<Stack>)>()
        .iter(world)
        .collect();

    if pending.is_empty() {
        return;
    }

    for stack in pending {
        let confirmed = show_close_dialog();

        if let Ok(mut entity_mut) = world.get_entity_mut(stack) {
            entity_mut.remove::<PendingStackClose>();
        }

        if !confirmed {
            continue;
        }

        let Some(parent_pane) = world.get::<ChildOf>(stack).map(|c| c.get()) else {
            continue;
        };

        let sibling_stacks: Vec<Entity> = world
            .get::<Children>(parent_pane)
            .map(|children| {
                children
                    .iter()
                    .filter(|&e| e != stack && world.get::<Stack>(e).is_some())
                    .collect()
            })
            .unwrap_or_default();

        let was_active = {
            let mut q = world.query::<(Entity, &LastActivatedAt)>();
            let stacks_with_ts: Vec<(Entity, LastActivatedAt)> = world
                .get::<Children>(parent_pane)
                .map(|children| {
                    children
                        .iter()
                        .filter_map(|e| q.get(world, e).ok())
                        .filter(|(e, _)| world.get::<Stack>(*e).is_some())
                        .map(|(e, ts)| (e, *ts))
                        .collect()
                })
                .unwrap_or_default();
            stacks_with_ts
                .iter()
                .max_by_key(|(_, ts)| ts.0)
                .map(|(e, _)| *e)
                == Some(stack)
        };

        world.despawn(stack);

        if was_active
            && let Some(&next) = sibling_stacks.first()
            && let Ok(mut entity_mut) = world.get_entity_mut(next)
        {
            entity_mut.insert(LastActivatedAt::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        settings::ConfirmCloseSettings,
        settings::{
            FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
        },
    };
    use bevy::window::ClosingWindow;
    use vmux_command::{CommandPlugin, WriteAppCommands};

    fn test_settings() -> LayoutSettings {
        LayoutSettings {
            radius: 0.0,
            window: WindowSettings {
                padding: 0.0,
                padding_top: None,
                padding_right: None,
                padding_bottom: None,
                padding_left: None,
            },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        }
    }

    fn place_pane(app: &mut App, parent: Entity, center: Vec2, size: Vec2) -> Entity {
        use bevy::ui::{ComputedNode, UiGlobalTransform};
        let node = ComputedNode { size, ..default() };
        let id = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(parent),
                node,
                UiGlobalTransform::from_translation(center),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(id)));
        id
    }

    #[test]
    fn select_right_picks_most_recently_active_among_overlapping_neighbors() {
        // Layout: A (left, full height), B (top-right), C (bottom-right).
        // From A, both B and C overlap on Y. Expect: navigate to whichever was
        // active most recently (B in this test).
        use bevy::ui::{ComputedNode, UiGlobalTransform};
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .add_systems(Update, on_pane_select.in_set(WriteAppCommands));

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split_v = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let a = place_pane(
            &mut app,
            split_v,
            Vec2::new(399.5, 450.0),
            Vec2::new(791.0, 892.0),
        );
        let split_h = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                ChildOf(split_v),
            ))
            .id();
        let b = place_pane(
            &mut app,
            split_h,
            Vec2::new(1199.5, 225.0),
            Vec2::new(793.0, 442.0),
        );
        let c = place_pane(
            &mut app,
            split_h,
            Vec2::new(1199.5, 675.0),
            Vec2::new(793.0, 442.0),
        );

        // Sanity: ensure ComputedNode is set for B and C
        let _ = app.world().get::<ComputedNode>(b).unwrap();
        let _ = app.world().get::<UiGlobalTransform>(b).unwrap();

        // Activate C first, then B (B is the most recently active right-side pane).
        app.world_mut().entity_mut(c).insert(LastActivatedAt::now());
        std::thread::sleep(std::time::Duration::from_millis(2));
        app.world_mut().entity_mut(b).insert(LastActivatedAt::now());
        // Then activate A so it's the current pane.
        std::thread::sleep(std::time::Duration::from_millis(2));
        app.world_mut().entity_mut(a).insert(LastActivatedAt::now());

        let prev_b = app.world().get::<LastActivatedAt>(b).unwrap().0;
        let prev_c = app.world().get::<LastActivatedAt>(c).unwrap().0;
        assert!(prev_b > prev_c, "B should be more recently active than C");

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectRight,
            )));
        app.update();

        let new_b = app.world().get::<LastActivatedAt>(b).unwrap().0;
        let new_c = app.world().get::<LastActivatedAt>(c).unwrap().0;
        assert!(
            new_b > prev_b,
            "B (most recently active) should be re-activated by SelectRight"
        );
        assert_eq!(new_c, prev_c, "C should not be re-activated");
    }

    #[test]
    fn select_left_picks_full_height_neighbor_from_sub_split_pane() {
        // Layout: A on left (full height), B top-right, C bottom-right.
        // From B, pressing 'h' should navigate to A (their bounding boxes overlap on Y).
        use bevy::ui::{ComputedNode, UiGlobalTransform};
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .add_systems(Update, on_pane_select.in_set(WriteAppCommands));

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split_v = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        // Realistic layout with gaps (8px pane gap, 4px window padding):
        // A: left, full height (791x892)
        let a = place_pane(
            &mut app,
            split_v,
            Vec2::new(399.5, 450.0),
            Vec2::new(791.0, 892.0),
        );
        let split_h = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                ChildOf(split_v),
            ))
            .id();
        // B: top-right, half height (793x442)
        let b = place_pane(
            &mut app,
            split_h,
            Vec2::new(1199.5, 225.0),
            Vec2::new(793.0, 442.0),
        );
        // C: bottom-right, half height (793x442)
        let _c = place_pane(
            &mut app,
            split_h,
            Vec2::new(1199.5, 675.0),
            Vec2::new(793.0, 442.0),
        );

        let _ = (a, b);
        // sanity: ensure ComputedNode is set
        let _ = app.world().get::<ComputedNode>(b).unwrap();
        let _ = app.world().get::<UiGlobalTransform>(b).unwrap();

        app.world_mut().entity_mut(a).insert(LastActivatedAt(1));
        app.world_mut().entity_mut(b).insert(LastActivatedAt(10));
        app.world_mut().entity_mut(_c).insert(LastActivatedAt(0));
        let prev_a = app.world().get::<LastActivatedAt>(a).unwrap().0;

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft,
            )));
        app.update();

        let new_a = app.world().get::<LastActivatedAt>(a).unwrap().0;
        assert!(
            new_a > prev_a,
            "SelectLeft from B should navigate to A (full-height left neighbor)"
        );
    }

    #[test]
    fn select_left_picks_left_neighbor_in_horizontal_split() {
        use bevy::ui::{ComputedNode, UiGlobalTransform};
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .add_systems(Update, on_pane_select.in_set(WriteAppCommands));

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let left = place_pane(
            &mut app,
            split,
            Vec2::new(400.0, 450.0),
            Vec2::new(800.0, 900.0),
        );
        let right = place_pane(
            &mut app,
            split,
            Vec2::new(1200.0, 450.0),
            Vec2::new(800.0, 900.0),
        );

        // make `right` the active pane
        app.world_mut()
            .entity_mut(right)
            .insert(LastActivatedAt::now());
        std::thread::sleep(std::time::Duration::from_millis(2));

        // sanity: ensure ComputedNode is set as expected
        assert_eq!(
            app.world().get::<ComputedNode>(right).unwrap().size,
            Vec2::new(800.0, 900.0)
        );
        assert_eq!(
            app.world()
                .get::<UiGlobalTransform>(right)
                .unwrap()
                .transform_point2(Vec2::ZERO),
            Vec2::new(1200.0, 450.0)
        );

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft,
            )));
        app.update();

        let new_active_left = app
            .world()
            .get::<LastActivatedAt>(left)
            .map(|t| t.0)
            .expect("left has LastActivatedAt");
        let prev_active_right = app
            .world()
            .get::<LastActivatedAt>(right)
            .map(|t| t.0)
            .expect("right has LastActivatedAt");
        assert!(
            new_active_left > prev_active_right,
            "SelectLeft should mark left as more recently activated than right"
        );
    }

    #[test]
    fn closing_last_pane_marks_window_closing() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .init_resource::<ConfirmCloseSettings>()
            .add_message::<PageOpenRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_pane_commands.in_set(WriteAppCommands));

        let window = app.world_mut().spawn(PrimaryWindow).id();
        let tab_e = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab_e)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close)));

        app.update();

        assert!(app.world().entity(window).contains::<ClosingWindow>());
    }

    #[test]
    fn split_gap_only_applies_on_split_axis() {
        let row = pane_split_gaps(PaneSplitDirection::Row, 8.0);
        let column = pane_split_gaps(PaneSplitDirection::Column, 8.0);

        assert_eq!(row.column_gap, Val::Px(8.0));
        assert_eq!(row.row_gap, Val::Px(0.0));
        assert_eq!(column.column_gap, Val::Px(0.0));
        assert_eq!(column.row_gap, Val::Px(8.0));
    }

    #[test]
    fn zoomed_component_constructs_and_reads_back() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let leaf = app.world_mut().spawn(Pane).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Zoomed {
                    leaf,
                    hidden: vec![],
                },
            ))
            .id();

        let z = app.world().get::<Zoomed>(tab).expect("Zoomed present");
        assert_eq!(z.leaf, leaf);
        assert!(z.hidden.is_empty());
    }

    #[test]
    fn zoom_command_inserts_zoomed_with_correct_hidden_set() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
        register_zoom_hooks(&mut app);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let leaf_a = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        let leaf_b = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
        app.world_mut()
            .entity_mut(leaf_b)
            .insert(LastActivatedAt::now());

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));

        app.update();

        let z = app
            .world()
            .get::<Zoomed>(tab)
            .expect("Zoomed inserted on tab");
        assert_eq!(z.leaf, leaf_b);
        assert_eq!(z.hidden, vec![leaf_a]);
    }

    #[test]
    fn zoom_command_on_zoomed_tab_removes_zoomed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
        register_zoom_hooks(&mut app);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let leaf_a = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        let leaf_b = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
        app.world_mut()
            .entity_mut(leaf_b)
            .insert(LastActivatedAt::now());

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
        app.update();
        assert!(app.world().get::<Zoomed>(tab).is_some());

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
        app.update();
        assert!(app.world().get::<Zoomed>(tab).is_none());
    }

    #[test]
    fn zoom_command_on_single_pane_tab_is_noop() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
        register_zoom_hooks(&mut app);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let only = app
            .world_mut()
            .spawn((Pane, Node::default(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(only)));

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
        app.update();

        assert!(app.world().get::<Zoomed>(tab).is_none());
    }

    #[test]
    fn removing_zoomed_pane_clears_zoom_state() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        register_zoom_hooks(&mut app);
        app.add_systems(PostUpdate, clear_zoom_on_pane_removal);

        let leaf_a = app.world_mut().spawn((Pane, Node::default())).id();
        let leaf_b = app.world_mut().spawn((Pane, Node::default())).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Zoomed {
                    leaf: leaf_b,
                    hidden: vec![leaf_a],
                },
            ))
            .id();

        app.update();

        app.world_mut().despawn(leaf_b);
        app.update();

        assert!(
            app.world().get::<Zoomed>(tab).is_none(),
            "Zoomed should be cleared when its leaf is despawned"
        );
    }

    #[test]
    fn split_command_auto_unzooms_first() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
        register_zoom_hooks(&mut app);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let leaf_a = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        let leaf_b = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
        app.world_mut()
            .entity_mut(leaf_b)
            .insert(LastActivatedAt::now());

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
        app.update();
        assert!(app.world().get::<Zoomed>(tab).is_some());

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPane {
                    direction: PaneDirection::Bottom,
                    target: PaneTarget::NewSplit,
                    mode: PaneOpenMode::NewStack,
                    url: None,
                },
            )));
        app.update();

        assert!(
            app.world().get::<Zoomed>(tab).is_none(),
            "open-in-pane should auto-unzoom"
        );
    }

    #[test]
    fn select_command_auto_unzooms() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
        register_zoom_hooks(&mut app);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let leaf_a = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        let leaf_b = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
        app.world_mut()
            .entity_mut(leaf_b)
            .insert(LastActivatedAt::now());

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
        app.update();
        assert!(app.world().get::<Zoomed>(tab).is_some());

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft,
            )));
        app.update();

        assert!(
            app.world().get::<Zoomed>(tab).is_none(),
            "navigation should auto-unzoom"
        );
    }

    #[test]
    fn removing_zoomed_restores_display_flex() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        register_zoom_hooks(&mut app);

        let leaf = app.world_mut().spawn((Pane, Node::default())).id();
        let sib = app
            .world_mut()
            .spawn((
                Pane,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Zoomed {
                    leaf,
                    hidden: vec![sib],
                },
            ))
            .id();

        app.update();

        app.world_mut().entity_mut(tab).remove::<Zoomed>();
        app.update();

        assert_eq!(app.world().get::<Node>(sib).unwrap().display, Display::Flex);
        let _ = leaf;
    }

    #[test]
    fn sync_zoom_visibility_sets_display_none_on_hidden_entities() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_zoom_visibility);

        let leaf = app.world_mut().spawn((Pane, Node::default())).id();
        let sib_a = app.world_mut().spawn((Pane, Node::default())).id();
        let sib_b = app.world_mut().spawn((Pane, Node::default())).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Zoomed {
                    leaf,
                    hidden: vec![sib_a, sib_b],
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<Node>(sib_a).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(sib_b).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(leaf).unwrap().display,
            Display::Flex
        );

        let _ = tab;
    }

    #[test]
    fn siblings_to_hide_collects_sibling_at_each_split_ancestor() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let tab = app.world_mut().spawn(Tab::default()).id();
        let split_root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let left = app.world_mut().spawn((Pane, ChildOf(split_root))).id();
        let right_split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                ChildOf(split_root),
            ))
            .id();
        let right_top = app.world_mut().spawn((Pane, ChildOf(right_split))).id();
        let right_bot = app.world_mut().spawn((Pane, ChildOf(right_split))).id();

        let result = {
            let world = app.world();
            siblings_to_hide(world, right_top, tab)
        };

        assert_eq!(result.len(), 2);
        assert!(result.contains(&right_bot));
        assert!(result.contains(&left));
    }

    #[test]
    fn siblings_to_hide_is_empty_for_single_pane_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let tab = app.world_mut().spawn(Tab::default()).id();
        let only = app.world_mut().spawn((Pane, ChildOf(tab))).id();

        let result = {
            let world = app.world();
            siblings_to_hide(world, only, tab)
        };

        assert!(result.is_empty());
    }

    #[test]
    fn pane_split_gap_sync_clears_cross_axis_gap() {
        let split = PaneSplit {
            direction: PaneSplitDirection::Row,
        };
        let mut node = Node {
            column_gap: Val::Px(16.0),
            row_gap: Val::Px(16.0),
            ..default()
        };

        apply_pane_split_gaps(&split, &mut node, 8.0);

        assert_eq!(node.column_gap, Val::Px(8.0));
        assert_eq!(node.row_gap, Val::Px(0.0));
    }

    #[derive(Resource, Default)]
    struct InPaneCollectedSpawns(Vec<PageOpenRequest>);

    fn collect_in_pane_spawns(
        mut reader: MessageReader<PageOpenRequest>,
        mut collected: ResMut<InPaneCollectedSpawns>,
    ) {
        for req in reader.read() {
            collected.0.push(req.clone());
        }
    }

    fn build_in_pane_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<InPaneCollectedSpawns>()
            .insert_resource(test_settings())
            .add_systems(
                Update,
                (
                    handle_open_in_pane.in_set(WriteAppCommands),
                    collect_in_pane_spawns.after(handle_open_in_pane),
                ),
            );
        let _window = app.world_mut().spawn(PrimaryWindow).id();
        app
    }

    fn build_single_pane(app: &mut App) -> (Entity, Entity, Entity) {
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        (tab, pane, stack)
    }

    fn build_pre_split(app: &mut App) -> (Entity, Entity, Entity, Entity) {
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneSize::default(),
                Node {
                    flex_grow: 1.0,
                    flex_direction: bevy::ui::FlexDirection::Row,
                    ..default()
                },
                ChildOf(tab),
            ))
            .id();
        let left = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt(10), ChildOf(split)))
            .id();
        let right = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt(5), ChildOf(split)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(10), ChildOf(left)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(5), ChildOf(right)));
        (tab, split, left, right)
    }

    #[test]
    fn find_sibling_pane_returns_right_neighbor() {
        use bevy_ecs::system::RunSystemOnce;
        use vmux_command::open::PaneDirection;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneSize::default(),
                Node::default(),
                ChildOf(tab),
            ))
            .id();
        let left = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(split)))
            .id();
        let right = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(split)))
            .id();

        app.update();

        let has_children = app.world().get::<Children>(split).is_some();
        assert!(has_children, "split entity should have Children component");
        let children: Vec<Entity> = app.world().get::<Children>(split).unwrap().iter().collect();
        assert!(
            children.contains(&left),
            "split children should contain left"
        );
        assert!(
            children.contains(&right),
            "split children should contain right"
        );

        let result = app.world_mut().run_system_once(
            move |child_of_q: Query<&ChildOf>,
                  split_dir_q: Query<&PaneSplit>,
                  pane_children: Query<&Children, With<Pane>>,
                  leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>| {
                find_sibling_pane(
                    left,
                    &PaneDirection::Right,
                    &child_of_q,
                    &split_dir_q,
                    &pane_children,
                    &leaf_panes,
                )
            },
        );

        assert_eq!(result.unwrap(), Some(right));
    }

    #[test]
    fn find_sibling_pane_returns_none_for_single_pane() {
        use bevy_ecs::system::RunSystemOnce;
        use vmux_command::open::PaneDirection;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();

        app.update();

        let result = app.world_mut().run_system_once(
            move |child_of_q: Query<&ChildOf>,
                  split_dir_q: Query<&PaneSplit>,
                  pane_children: Query<&Children, With<Pane>>,
                  leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>| {
                find_sibling_pane(
                    pane,
                    &PaneDirection::Right,
                    &child_of_q,
                    &split_dir_q,
                    &pane_children,
                    &leaf_panes,
                )
            },
        );

        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn in_pane_new_split_right_creates_pane_to_the_right() {
        use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
        let mut app = build_in_pane_app();
        let (_tab, pane, _stack) = build_single_pane(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPane {
                    direction: PaneDirection::Right,
                    target: PaneTarget::NewSplit,
                    mode: PaneOpenMode::NewStack,
                    url: Some("https://x".into()),
                },
            )));
        app.update();

        assert!(
            app.world().get::<PaneSplit>(pane).is_some(),
            "original pane should now be a split"
        );
        let ps = app.world().get::<PaneSplit>(pane).unwrap();
        assert_eq!(ps.direction, PaneSplitDirection::Row);

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(pane)
            .unwrap()
            .iter()
            .filter(|e| app.world().get::<Pane>(*e).is_some())
            .collect();
        assert_eq!(children.len(), 2, "should have two child panes");

        let collected = app.world().resource::<InPaneCollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        match &collected.0[0] {
            PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url,
                ..
            } => {
                assert_eq!(url, "https://x");
                let stack_parent = app.world().get::<ChildOf>(*stack).map(|c| c.get()).unwrap();
                assert_eq!(
                    stack_parent, children[1],
                    "new stack should be in the second (right) pane"
                );
            }
            other => panic!("expected PageOpenRequest, got {other:?}"),
        }
    }

    #[test]
    fn in_pane_existing_in_place_navigates_neighbor_active_stack() {
        use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
        let mut app = build_in_pane_app();
        let (_tab, _split, _left, right) = build_pre_split(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPane {
                    direction: PaneDirection::Right,
                    target: PaneTarget::Existing,
                    mode: PaneOpenMode::InPlace,
                    url: Some("https://new".into()),
                },
            )));
        app.update();

        let collected = app.world().resource::<InPaneCollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        match &collected.0[0] {
            PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url,
                ..
            } => {
                assert_eq!(url, "https://new");
                let stack_parent = app.world().get::<ChildOf>(*stack).map(|c| c.get()).unwrap();
                assert_eq!(
                    stack_parent, right,
                    "should navigate the existing right pane's stack"
                );
            }
            other => panic!("expected PageOpenRequest, got {other:?}"),
        }
    }

    #[test]
    fn in_pane_existing_new_stack_adds_stack_to_neighbor() {
        use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
        let mut app = build_in_pane_app();
        let (_tab, _split, _left, right) = build_pre_split(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPane {
                    direction: PaneDirection::Right,
                    target: PaneTarget::Existing,
                    mode: PaneOpenMode::NewStack,
                    url: Some("https://x".into()),
                },
            )));
        app.update();

        let collected = app.world().resource::<InPaneCollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        let new_stack = match &collected.0[0] {
            PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url,
                ..
            } => {
                assert_eq!(url, "https://x");
                let stack_parent = app.world().get::<ChildOf>(*stack).map(|c| c.get()).unwrap();
                assert_eq!(stack_parent, right);
                *stack
            }
            other => panic!("expected PageOpenRequest, got {other:?}"),
        };

        app.update();

        let right_stacks: Vec<Entity> = app
            .world()
            .get::<Children>(right)
            .map(|c| {
                c.iter()
                    .filter(|e| app.world().get::<Stack>(*e).is_some())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(right_stacks.len(), 2, "right pane should now have 2 stacks");
        assert!(right_stacks.contains(&new_stack));
    }

    #[test]
    fn in_pane_existing_falls_back_to_new_split_when_no_sibling() {
        use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
        let mut app = build_in_pane_app();
        let (_tab, pane, _stack) = build_single_pane(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPane {
                    direction: PaneDirection::Right,
                    target: PaneTarget::Existing,
                    mode: PaneOpenMode::InPlace,
                    url: Some("https://x".into()),
                },
            )));
        app.update();

        assert!(
            app.world().get::<PaneSplit>(pane).is_some(),
            "should have fallen back to splitting"
        );

        let collected = app.world().resource::<InPaneCollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        assert_eq!(collected.0[0].url, "https://x");
    }
}
