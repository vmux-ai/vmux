use crate::{
    CloseRequiresConfirmation, NewStackContext,
    settings::{ConfirmCloseSettings, LayoutSettings},
    stack::{
        ActiveTabParam, CloseConfirmed, PendingStackClose, Stack, active_among, active_pane_in_tab,
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
    window::PrimaryWindow,
};
use bevy_cef::prelude::CefKeyboardTarget;
use moonshine_save::prelude::*;
use std::time::Instant;
use vmux_command::{
    AppCommand, BrowserCommand, LayoutCommand, OpenCommand, PaneCommand, ReadAppCommands,
    open::{PaneDirection, PaneOpenMode, PaneTarget},
};
use vmux_core::{PageOpenRequest, PageOpenTarget, PageOpenTask};
use vmux_history::LastActivatedAt;

/// Marker: pane is waiting for close confirmation dialog.
#[derive(Component)]
pub struct PendingPaneClose;

/// Marker: close this pane immediately, without a confirmation dialog. Used when
/// the pane's process has already exited (e.g. an agent CLI quit), so there is
/// nothing to confirm and the pane should be removed + the split collapsed.
#[derive(Component)]
pub struct ForcePaneClose;

pub struct PanePlugin;

#[cfg_attr(target_os = "macos", allow(dead_code))]
const HOVER_COOLDOWN_MS: u64 = 300;

#[derive(Resource, Default)]
pub struct PaneHoverIntent {
    pub target: Option<Entity>,
    pub last_activation: Option<Instant>,
}

impl Plugin for PanePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Pane>()
            .register_type::<PaneId>()
            .register_type::<PaneSplit>()
            .register_type::<PaneSplitDirection>()
            .register_type::<PaneSize>()
            .register_type::<SpawnSeq>()
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<SpawnCounter>()
            .add_systems(Update, stamp_spawn_seq)
            .add_systems(Update, assign_pane_ids)
            .add_systems(
                Startup,
                reseed_spawn_counter.in_set(crate::LayoutStartupSet::Post),
            )
            .add_systems(Update, on_pane_select.in_set(ReadAppCommands))
            .add_systems(Update, handle_pane_commands.in_set(ReadAppCommands))
            .add_systems(Update, handle_open_in_pane.in_set(ReadAppCommands))
            .add_message::<OpenBesideRequest>()
            .add_systems(Update, handle_open_beside_requests)
            .add_systems(
                Update,
                handle_zoom_command
                    .in_set(ReadAppCommands)
                    .before(handle_pane_commands),
            )
            .add_systems(
                Update,
                (
                    click_pane_in_player_mode,
                    pane_gap_drag_resize,
                    process_pending_pane_closes,
                    process_force_pane_closes,
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
        #[cfg(target_os = "macos")]
        app.add_systems(
            Update,
            (
                publish_pane_hover_regions,
                apply_pending_hover.before(crate::stack::ComputeFocusSet),
            ),
        );
        #[cfg(not(target_os = "macos"))]
        app.add_systems(
            Update,
            poll_cursor_pane_focus.before(crate::stack::ComputeFocusSet),
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

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct PaneId(pub String);

pub fn assign_pane_ids(
    panes: Query<Entity, (With<Pane>, Without<PaneId>)>,
    mut commands: Commands,
) {
    for entity in &panes {
        commands
            .entity(entity)
            .insert(PaneId(uuid::Uuid::new_v4().to_string()));
    }
}

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
    active_tab_param: ActiveTabParam,
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

#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct SpawnSeq(pub u64);

#[derive(Resource, Default)]
pub struct SpawnCounter(pub u64);

pub fn stamp_spawn_seq(
    mut counter: ResMut<SpawnCounter>,
    new_panes: Query<Entity, (With<Pane>, Without<SpawnSeq>)>,
    mut commands: Commands,
) {
    for pane in &new_panes {
        counter.0 += 1;
        commands.entity(pane).insert(SpawnSeq(counter.0));
    }
}

pub fn reseed_spawn_counter(seqs: Query<&SpawnSeq>, mut counter: ResMut<SpawnCounter>) {
    let max = seqs.iter().map(|s| s.0).max().unwrap_or(0);
    if counter.0 <= max {
        counter.0 = max + 1;
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

pub(crate) fn set_pane_split_direction(
    world: &mut World,
    entity: Entity,
    direction: PaneSplitDirection,
) {
    if let Some(mut split) = world.get_mut::<PaneSplit>(entity) {
        split.direction = direction;
    }
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.flex_direction = match direction {
            PaneSplitDirection::Row => FlexDirection::Row,
            PaneSplitDirection::Column => FlexDirection::Column,
        };
        let gaps = pane_split_gaps(direction, crate::event::PANE_GAP_PX);
        node.column_gap = gaps.column_gap;
        node.row_gap = gaps.row_gap;
    }
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
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    child_of_q: Query<&ChildOf>,
    split_dir_q: Query<&PaneSplit>,
    tab_filter: Query<Entity, With<Stack>>,
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
                    continue;
                }

                let Ok(siblings) = pane_children.get(parent) else {
                    continue;
                };
                let pane_siblings: Vec<Entity> = siblings
                    .iter()
                    .filter(|&e| e != active && (leaf_panes.contains(e) || split_dir_q.contains(e)))
                    .collect();

                if pane_siblings.len() >= 2 {
                    commands.entity(active).despawn();
                    let new_active_pane = pane_siblings
                        .iter()
                        .copied()
                        .max_by_key(|&e| pane_ts.get(e).map(|(_, t)| t.0).unwrap_or(0))
                        .unwrap_or(pane_siblings[0]);
                    let focus_leaf =
                        first_leaf_descendant(new_active_pane, &pane_children, &leaf_panes);
                    commands.entity(focus_leaf).insert(LastActivatedAt::now());
                    if let Some(stack) = active_stack_in_pane(focus_leaf, &pane_children, &stack_ts)
                        .or_else(|| first_stack_in_pane(focus_leaf, &pane_children, &tab_filter))
                    {
                        commands.entity(stack).insert(LastActivatedAt::now());
                    }
                    continue;
                }

                let Some(sibling) = pane_siblings.into_iter().next() else {
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
                    let sibling_direction = split_dir_q
                        .get(sibling)
                        .map(|s| s.direction)
                        .unwrap_or_default();
                    new_active_pane = first_leaf_descendant(sibling, &pane_children, &leaf_panes);
                    commands.entity(sibling).remove::<ChildOf>();
                    commands.queue(move |world: &mut World| {
                        world.despawn(sibling);
                        set_pane_split_direction(world, parent, sibling_direction);
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

pub fn direction_to_split(direction: &PaneDirection) -> PaneSplitDirection {
    match direction {
        PaneDirection::Left | PaneDirection::Right => PaneSplitDirection::Row,
        PaneDirection::Top | PaneDirection::Bottom => PaneSplitDirection::Column,
    }
}

pub fn split_leaf_into_two(
    commands: &mut Commands,
    active: Entity,
    split_dir: PaneSplitDirection,
    existing_tabs: &[Entity],
    activate_new: bool,
) -> Entity {
    split_leaf_into_two_parts(commands, active, split_dir, existing_tabs, activate_new).1
}

fn split_leaf_into_two_parts(
    commands: &mut Commands,
    active: Entity,
    split_dir: PaneSplitDirection,
    existing_tabs: &[Entity],
    activate_new: bool,
) -> (Entity, Entity) {
    let new_ts = if activate_new {
        LastActivatedAt::now()
    } else {
        LastActivatedAt(0)
    };
    let pane1 = commands
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(active)))
        .id();
    let p2 = commands
        .spawn((leaf_pane_bundle(), new_ts, ChildOf(active)))
        .id();
    for tab in existing_tabs {
        commands.entity(*tab).insert(ChildOf(pane1));
    }
    commands.entity(active).insert(split_root_bundle(split_dir));
    (pane1, p2)
}

/// Return a fresh empty leaf pane beside `anchor`, to host an agent-spawned
/// terminal. When `anchor` is still a leaf (`already_split == false`), it is
/// split in two via [`split_leaf_into_two`] (its stacks move into the first
/// child, the returned pane is the second). When `anchor` is already a split —
/// either from a previous frame, or from an earlier call in the *same* command
/// buffer — the new leaf is appended as another child of that split.
///
/// Calling [`split_leaf_into_two`] repeatedly on the same leaf within one
/// command buffer (e.g. several agent `run`s dispatched in one tick) would wrap
/// it again on each call and orphan an empty `pane1` every time; routing the
/// 2nd+ split through here keeps the result a clean N-ary split with no empties.
pub fn split_or_extend(
    commands: &mut Commands,
    anchor: Entity,
    split_dir: PaneSplitDirection,
    existing_tabs: &[Entity],
    activate_new: bool,
    already_split: bool,
) -> Entity {
    if already_split {
        let ts = if activate_new {
            LastActivatedAt::now()
        } else {
            LastActivatedAt(0)
        };
        commands
            .spawn((leaf_pane_bundle(), ts, ChildOf(anchor)))
            .id()
    } else {
        split_leaf_into_two(commands, anchor, split_dir, existing_tabs, activate_new)
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
pub struct ResolverCtx<'w, 's> {
    all_children: Query<'w, 's, &'static Children>,
    seq_q: Query<'w, 's, &'static SpawnSeq>,
    node_q: Query<'w, 's, &'static ComputedNode>,
    page_q: Query<'w, 's, &'static vmux_core::PageMetadata, With<Stack>>,
    open_task_q: Query<'w, 's, &'static PageOpenTask>,
    spaces: Query<'w, 's, (), With<crate::space::Space>>,
    tab_q: Query<'w, 's, Entity, With<Tab>>,
}

pub fn handle_open_beside_requests(
    mut reader: MessageReader<OpenBesideRequest>,
    pane_children: Query<&Children, With<Pane>>,
    split_dir_q: Query<&PaneSplit>,
    tab_filter: Query<Entity, With<Stack>>,
    child_of_q: Query<&ChildOf>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    rc: ResolverCtx,
    mut commands: Commands,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut new_stack_ctx: ResMut<NewStackContext>,
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
                &mut new_stack_ctx,
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
                &mut new_stack_ctx,
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
                    &mut new_stack_ctx,
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
                    &mut new_stack_ctx,
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
        split_leaf_into_two_parts(commands, anchor, split_dir, existing_tabs, activate_new);
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
    new_stack_ctx: &mut NewStackContext,
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
    open_or_prompt_stack(
        new_stack,
        Some(req.url.clone()),
        new_stack_ctx,
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
    mut reader: MessageReader<AppCommand>,
    active_tab_param: ActiveTabParam,
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
    mut new_stack_ctx: ResMut<NewStackContext>,
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

        let resolved = vmux_command::open::handler::resolve_url(
            url.as_deref(),
            effective_startup_url.as_ref().map(|s| s.0.as_str()),
        );
        let resolved = (!resolved.is_empty()).then_some(resolved);

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
            open_or_prompt_stack(
                new_stack,
                resolved,
                &mut new_stack_ctx,
                &mut page_open_requests,
            );
        } else {
            match mode {
                PaneOpenMode::InPlace => {
                    let active_stack = active_stack_in_pane(target_pane, &pane_children, &stack_ts)
                        .or_else(|| first_stack_in_pane(target_pane, &pane_children, &tab_filter));
                    if let Some(stack) = active_stack {
                        open_or_prompt_stack(
                            stack,
                            resolved,
                            &mut new_stack_ctx,
                            &mut page_open_requests,
                        );
                    }
                }
                PaneOpenMode::NewStack => {
                    let new_stack = commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(target_pane)))
                        .id();
                    open_or_prompt_stack(
                        new_stack,
                        resolved,
                        &mut new_stack_ctx,
                        &mut page_open_requests,
                    );
                }
            }
        }
        pending_warp.target = Some(target_pane);
    }
}

fn open_or_prompt_stack(
    stack: Entity,
    url: Option<String>,
    new_stack_ctx: &mut NewStackContext,
    page_open_requests: &mut MessageWriter<PageOpenRequest>,
) {
    if let Some(url) = url {
        new_stack_ctx.stack = None;
        new_stack_ctx.previous_stack = None;
        new_stack_ctx.needs_open = false;
        page_open_requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url,
            request_id: None,
        });
    } else {
        new_stack_ctx.stack = Some(stack);
        new_stack_ctx.previous_stack = None;
        new_stack_ctx.needs_open = true;
    }
}

fn on_pane_select(
    mut reader: MessageReader<AppCommand>,
    active_tab_param: ActiveTabParam,
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

        let active_tab = active_tab_param.get();
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

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn poll_cursor_pane_focus(
    mode: Res<crate::scene::InteractionMode>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    leaf_panes: Query<
        (Entity, &ComputedNode, &UiGlobalTransform),
        (With<Pane>, Without<PaneSplit>),
    >,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
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
    let Ok((window_entity, window)) = windows.single() else {
        return;
    };
    let Some(cursor) = pane_hover_cursor_position(window_entity, window) else {
        return;
    };

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

    commands.entity(target).insert(LastActivatedAt::now());
    if let Some(target_stack) = active_stack_in_pane(target, &pane_children, &stack_ts) {
        commands.entity(target_stack).insert(LastActivatedAt::now());
    }
    intent.target = None;
}

pub fn pane_hover_cursor_position(window_entity: Entity, window: &Window) -> Option<Vec2> {
    #[cfg(target_os = "macos")]
    {
        native_window_cursor_position(window_entity, window).or_else(|| {
            window
                .physical_cursor_position()
                .map(|pos| Vec2::new(pos.x, pos.y))
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window_entity;
        window
            .physical_cursor_position()
            .map(|pos| Vec2::new(pos.x, pos.y))
    }
}

#[cfg(target_os = "macos")]
fn native_window_cursor_position(window_entity: Entity, window: &Window) -> Option<Vec2> {
    use bevy::winit::WINIT_WINDOWS;
    use objc2_app_kit::{NSApplication, NSEvent, NSView};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let mtm = objc2::MainThreadMarker::new()?;
        if !NSApplication::sharedApplication(mtm).isActive() {
            return None;
        }
        let winit_window = winit_windows.get_window(window_entity)?;
        let handle = winit_window.window_handle().ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return None;
        };
        let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
        let ns_window = view.window()?;
        let screen_point = NSEvent::mouseLocation();
        let window_point = ns_window.convertPointFromScreen(screen_point);
        let point = view.convertPoint_fromView(window_point, None);
        let bounds = view.bounds();
        let y = if view.isFlipped() {
            point.y
        } else {
            bounds.size.height - point.y
        };
        let scale = window.resolution.scale_factor() as f64;
        let x = point.x * scale;
        let y = y * scale;
        if x.is_finite() && y.is_finite() {
            Some(Vec2::new(x as f32, y as f32))
        } else {
            None
        }
    })
}

#[cfg(target_os = "macos")]
mod hover_wake {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{LazyLock, Mutex};

    #[derive(Default)]
    struct Regions {
        panes: Vec<(u64, f32, f32, f32, f32)>,
        active: Option<u64>,
    }

    static REGIONS: LazyLock<Mutex<Regions>> = LazyLock::new(|| Mutex::new(Regions::default()));
    static PENDING: AtomicU64 = AtomicU64::new(0);

    pub fn publish(panes: Vec<(u64, f32, f32, f32, f32)>, active: Option<u64>) {
        if let Ok(mut regions) = REGIONS.lock() {
            regions.panes = panes;
            regions.active = active;
        }
    }

    fn region_contains(region: &(u64, f32, f32, f32, f32), x: f32, y: f32) -> bool {
        let (_, min_x, min_y, max_x, max_y) = *region;
        x >= min_x && x <= max_x && y >= min_y && y <= max_y
    }

    pub fn wake_on_move(x: f32, y: f32) -> bool {
        let Ok(regions) = REGIONS.lock() else {
            return false;
        };
        let hit = regions
            .panes
            .iter()
            .find(|region| region_contains(region, x, y))
            .map(|(entity, ..)| *entity);
        if let Some(entity) = hit
            && Some(entity) != regions.active
        {
            PENDING.store(entity, Ordering::Relaxed);
        }
        true
    }

    pub fn cursor_over_pane(x: f32, y: f32) -> bool {
        let Ok(regions) = REGIONS.lock() else {
            return false;
        };
        regions
            .panes
            .iter()
            .any(|region| region_contains(region, x, y))
    }

    pub fn take_pending_target() -> Option<u64> {
        match PENDING.swap(0, Ordering::Relaxed) {
            0 => None,
            bits => Some(bits),
        }
    }
}

#[cfg(target_os = "macos")]
pub use hover_wake::{cursor_over_pane, wake_on_move};

#[cfg(target_os = "macos")]
fn publish_pane_hover_regions(
    mode: Res<crate::scene::InteractionMode>,
    leaf_panes: Query<
        (Entity, &ComputedNode, &UiGlobalTransform),
        (With<Pane>, Without<PaneSplit>),
    >,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
) {
    if *mode != crate::scene::InteractionMode::User {
        hover_wake::publish(Vec::new(), None);
        return;
    }
    let mut panes = Vec::new();
    for (entity, node, ui_gt) in &leaf_panes {
        let center = ui_gt.transform_point2(Vec2::ZERO);
        let half = node.size * 0.5;
        panes.push((
            entity.to_bits(),
            center.x - half.x,
            center.y - half.y,
            center.x + half.x,
            center.y + half.y,
        ));
    }
    let active = active_among(
        leaf_panes
            .iter()
            .filter_map(|(entity, _, _)| pane_ts.get(entity).ok()),
    )
    .map(|entity| entity.to_bits());
    hover_wake::publish(panes, active);
}

#[cfg(target_os = "macos")]
fn apply_pending_hover(
    mode: Res<crate::scene::InteractionMode>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
) {
    let Some(bits) = hover_wake::take_pending_target() else {
        return;
    };
    if *mode != crate::scene::InteractionMode::User {
        return;
    }
    let Some(target) = Entity::try_from_bits(bits) else {
        return;
    };
    if !leaf_panes.contains(target) {
        return;
    }
    let current = active_among(leaf_panes.iter().filter_map(|e| pane_ts.get(e).ok()));
    if current == Some(target) {
        return;
    }
    commands.entity(target).insert(LastActivatedAt::now());
    if let Some(stack) = active_stack_in_pane(target, &pane_children, &stack_ts) {
        commands.entity(stack).insert(LastActivatedAt::now());
    }
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

/// Exclusive system: force-close panes marked [`ForcePaneClose`] with no
/// confirmation dialog. Mirrors [`process_pending_pane_closes`] (activate the
/// pane + its tab, mark `CloseConfirmed`, dispatch `PaneCommand::Close`) but
/// skips the prompt, since the process has already exited. Being exclusive, the
/// activation lands before the dispatched command is read.
fn process_force_pane_closes(world: &mut World) {
    let pending: Vec<Entity> = world
        .query_filtered::<Entity, (With<ForcePaneClose>, With<Pane>)>()
        .iter(world)
        .collect();

    if pending.is_empty() {
        return;
    }

    for pane in pending {
        let Ok(mut entity_mut) = world.get_entity_mut(pane) else {
            continue;
        };
        entity_mut.remove::<ForcePaneClose>();
        entity_mut.insert((CloseConfirmed, LastActivatedAt::now()));

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
            window: WindowSettings { padding: 0.0 },
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
    fn stamp_spawn_seq_assigns_increasing_values_to_new_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SpawnCounter>()
            .add_systems(Update, stamp_spawn_seq);

        let a = app.world_mut().spawn(Pane).id();
        app.update();
        let b = app.world_mut().spawn(Pane).id();
        app.update();

        let sa = app.world().get::<SpawnSeq>(a).expect("a stamped").0;
        let sb = app.world().get::<SpawnSeq>(b).expect("b stamped").0;
        assert!(
            sb > sa,
            "later-created pane must have higher SpawnSeq ({sb} > {sa})"
        );
    }

    #[test]
    fn reseed_spawn_counter_exceeds_max_existing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SpawnCounter>()
            .add_systems(Update, reseed_spawn_counter);

        app.world_mut().spawn((Pane, SpawnSeq(7)));
        app.world_mut().spawn((Pane, SpawnSeq(3)));
        app.update();

        assert_eq!(app.world().resource::<SpawnCounter>().0, 8);
    }

    #[test]
    fn open_beside_reuses_sibling_pane_as_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<SpawnCounter>()
            .add_systems(Update, handle_open_beside_requests);

        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
            ))
            .id();
        let agent_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(split)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            LastActivatedAt::now(),
            ChildOf(agent_pane),
        ));
        let other_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(split)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: Some(PaneDirection::Right),
                url: "file:///x.rs".to_string(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(agent_pane).is_none(),
            "agent pane must not be split when a sibling exists"
        );
        let kids: Vec<Entity> = app
            .world()
            .get::<Children>(other_pane)
            .map(|c| c.iter().collect())
            .unwrap_or_default();
        let stacks = kids
            .into_iter()
            .filter(|&e| app.world().get::<Stack>(e).is_some())
            .count();
        assert_eq!(
            stacks, 1,
            "page should open as a new stack in the sibling pane"
        );
    }

    #[test]
    fn open_beside_splits_when_no_sibling() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<SpawnCounter>()
            .add_systems(Update, handle_open_beside_requests);

        let pane = app.world_mut().spawn((Pane, LastActivatedAt::now())).id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane,
                direction: Some(PaneDirection::Right),
                url: "file:///x.rs".to_string(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(pane).is_some(),
            "a lone pane should split when there is no sibling"
        );
    }

    fn place_pane_with_url(
        app: &mut App,
        parent: Entity,
        seq: u64,
        size: Vec2,
        url: &str,
    ) -> Entity {
        use bevy::ui::{ComputedNode, UiGlobalTransform};
        let pane = app
            .world_mut()
            .spawn((
                Pane,
                SpawnSeq(seq),
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(parent),
                ComputedNode { size, ..default() },
                UiGlobalTransform::from_translation(size * 0.5),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        app.world_mut()
            .entity_mut(stack)
            .insert(vmux_core::PageMetadata {
                url: url.to_string(),
                ..default()
            });
        pane
    }

    fn stack_in_pane(app: &App, pane: Entity) -> Entity {
        let stacks: Vec<Entity> = app
            .world()
            .get::<Children>(pane)
            .map(|c| {
                c.iter()
                    .filter(|&e| app.world().get::<Stack>(e).is_some())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(stacks.len(), 1, "expected one stack in pane");
        stacks[0]
    }

    fn page_open_requests(app: &App) -> Vec<PageOpenRequest> {
        let messages = app.world().resource::<Messages<PageOpenRequest>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).cloned().collect()
    }

    fn materialize_page_metadata(app: &mut App) {
        for request in page_open_requests(app) {
            if let PageOpenTarget::Stack(stack) = request.target {
                app.world_mut()
                    .entity_mut(stack)
                    .insert(vmux_core::PageMetadata {
                        url: request.url,
                        ..default()
                    });
            }
        }
    }

    fn open_beside_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<SpawnCounter>()
            .add_systems(Update, handle_open_beside_requests);
        app
    }

    #[test]
    fn auto_same_type_adds_tab_without_splitting() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 5, Vec2::new(800.0, 600.0), "https://a.com");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: browser_pane,
                direction: None,
                url: "https://b.com".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "same type must not split"
        );
        let stacks = app
            .world()
            .get::<Children>(browser_pane)
            .map(|c| {
                c.iter()
                    .filter(|&e| app.world().get::<Stack>(e).is_some())
                    .count()
            })
            .unwrap_or(0);
        assert_eq!(
            stacks, 2,
            "new browser page tabs into the existing browser pane"
        );
    }

    #[test]
    fn auto_batched_files_stack_in_first_file_pane() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(800.0, 900.0),
            "vmux://agent/claude/session",
        );
        place_pane_with_url(
            &mut app,
            tab,
            2,
            Vec2::new(900.0, 400.0),
            "vmux://terminal/123",
        );

        for url in ["file:///repo/a.rs", "file:///repo/b.rs"] {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [0u8; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        let file_stack_parents: Vec<Entity> = requests
            .iter()
            .filter_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url.starts_with("file:") => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .collect();

        assert_eq!(file_stack_parents.len(), 2);
        assert_eq!(
            file_stack_parents[0], file_stack_parents[1],
            "same-frame file opens should stack in one file pane"
        );
    }

    #[test]
    fn auto_batched_new_types_split_from_newest_target() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
            "vmux://terminal/",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        let parent_for = |prefix: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url.starts_with(prefix) => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let browser_parent = parent_for("https:");
        let file_parent = parent_for("file:");
        let terminal_parent = parent_for("vmux://terminal/");

        for parent in [browser_parent, file_parent, terminal_parent] {
            assert!(
                app.world().get::<PaneSplit>(parent).is_none(),
                "new stack must live in a leaf pane, not directly under a split"
            );
        }

        let browser_split = app.world().get::<ChildOf>(browser_parent).unwrap().get();
        let file_split = app.world().get::<ChildOf>(file_parent).unwrap().get();
        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            file_split
        );
        assert_eq!(
            app.world().get::<ChildOf>(file_split).unwrap().get(),
            browser_split
        );
        assert_eq!(
            app.world().get::<PaneSplit>(agent_pane).unwrap().direction,
            PaneSplitDirection::Row
        );
        assert_eq!(
            app.world()
                .get::<PaneSplit>(browser_split)
                .unwrap()
                .direction,
            PaneSplitDirection::Column
        );
        assert_eq!(
            app.world().get::<PaneSplit>(file_split).unwrap().direction,
            PaneSplitDirection::Row
        );
    }

    #[test]
    fn auto_batched_new_browser_stacks_in_existing_browser_bucket_after_other_work() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux/pull/221",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
            "file:///repo/crates/vmux_layout/src/pane.rs",
            "vmux://terminal/",
            "https://github.com/vmux-ai/vmux/actions/runs/28544986467",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let pr_parent = parent_for_url("https://github.com/vmux-ai/vmux/pull/221");
        let ci_parent = parent_for_url("https://github.com/vmux-ai/vmux/actions/runs/28544986467");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert_eq!(
            ci_parent, pr_parent,
            "new CI browser page should tab into the existing browser pane"
        );
        assert_ne!(ci_parent, terminal_parent);
    }

    #[test]
    fn auto_batched_new_browser_stacks_after_nonbrowser_tab_reuse() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux/pull/221",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
            "vmux://terminal/",
            "https://github.com/vmux-ai/vmux/actions/runs/28544986467",
            "file:///repo/crates/vmux_layout/src/pane.rs",
            "https://github.com/vmux-ai/vmux/pull/221/files",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let ci_parent = parent_for_url("https://github.com/vmux-ai/vmux/actions/runs/28544986467");
        let files_parent = parent_for_url("https://github.com/vmux-ai/vmux/pull/221/files");

        assert_eq!(
            files_parent, ci_parent,
            "browser pages after file tab reuse should stack in the newest browser pane"
        );
    }

    #[test]
    fn auto_file_bucket_stays_reusable_after_multiple_file_tabs() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux/pull/221",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
            "file:///repo/crates/vmux_layout/src/pane.rs",
            "vmux://terminal/",
            "file:///repo/crates/vmux_layout/src/placement.rs",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
            app.update();
            materialize_page_metadata(&mut app);
        }

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let plugin_parent = parent_for_url("file:///repo/crates/vmux_agent/src/plugin.rs");
        let pane_parent = parent_for_url("file:///repo/crates/vmux_layout/src/pane.rs");
        let placement_parent = parent_for_url("file:///repo/crates/vmux_layout/src/placement.rs");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert_eq!(pane_parent, plugin_parent);
        assert_eq!(
            placement_parent, plugin_parent,
            "later files should reuse the existing file pane even after it has multiple file tabs"
        );
        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            app.world().get::<ChildOf>(plugin_parent).unwrap().get(),
            "terminal should split the current file tail"
        );
    }

    #[test]
    fn auto_terminal_splits_current_file_tail_after_file_bucket_reuse() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux/pull/221",
            "file:///repo/crates/vmux_layout/src/pane.rs",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
            app.update();
            materialize_page_metadata(&mut app);
        }

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "vmux://terminal/".into(),
                request_id: [9; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let pr_parent = parent_for_url("https://github.com/vmux-ai/vmux/pull/221");
        let plugin_parent = parent_for_url("file:///repo/crates/vmux_agent/src/plugin.rs");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            app.world().get::<ChildOf>(plugin_parent).unwrap().get(),
            "terminal should split the current file tail"
        );
        assert_ne!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            app.world().get::<ChildOf>(pr_parent).unwrap().get()
        );
    }

    #[test]
    fn auto_first_file_splits_terminal_when_terminal_is_newer() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );
        let browser_pane = place_pane_with_url(
            &mut app,
            tab,
            10,
            Vec2::new(900.0, 400.0),
            "https://news.ycombinator.com/news",
        );
        let terminal_pane = place_pane_with_url(
            &mut app,
            tab,
            20,
            Vec2::new(900.0, 400.0),
            "vmux://terminal/",
        );

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "file:///repo/README.md".into(),
                request_id: [9; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let file_parent = requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == "file:///repo/README.md" => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap();

        assert_eq!(
            app.world().get::<ChildOf>(file_parent).unwrap().get(),
            terminal_pane,
            "first file should split the newest terminal pane"
        );
        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "browser pane must not split for first file"
        );
    }

    #[test]
    fn auto_browser_open_after_files_becomes_anchor_for_terminal() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        for (i, url) in [
            "file:///repo/.git/HEAD",
            "file:///repo/.git/refs/heads/main",
            "https://news.ycombinator.com/news",
            "vmux://terminal/",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
            app.update();
            materialize_page_metadata(&mut app);
        }

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let file_parent = parent_for_url("file:///repo/.git/refs/heads/main");
        let browser_parent = parent_for_url("https://news.ycombinator.com/news");
        let terminal_parent = parent_for_url("vmux://terminal/");
        let terminal_split = app.world().get::<ChildOf>(terminal_parent).unwrap().get();

        assert_eq!(
            terminal_split,
            app.world().get::<ChildOf>(browser_parent).unwrap().get(),
            "terminal should split the browser pane when the browser opened after files"
        );
        assert_ne!(
            terminal_split,
            app.world().get::<ChildOf>(file_parent).unwrap().get(),
            "terminal must not split the older file pane"
        );
    }

    #[test]
    fn auto_file_after_terminal_stacks_in_existing_file_bucket() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        for (i, url) in [
            "file:///repo/.git/HEAD",
            "file:///repo/.git/refs/heads/main",
            "https://news.ycombinator.com/news",
            "vmux://terminal/",
            "file:///repo/README.md",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
            app.update();
            materialize_page_metadata(&mut app);
        }

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let stale_file_parent = parent_for_url("file:///repo/.git/refs/heads/main");
        let terminal_parent = parent_for_url("vmux://terminal/");
        let readme_parent = parent_for_url("file:///repo/README.md");

        assert_eq!(
            readme_parent, stale_file_parent,
            "README should tab into the existing file pane"
        );
        assert_ne!(
            readme_parent, terminal_parent,
            "README must not tab into the terminal pane"
        );
    }

    #[test]
    fn auto_duplicate_url_reuses_pending_open_in_same_batch() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        for i in 0..2 {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: "https://github.com/vmux-ai/vmux/pull/221".into(),
                    request_id: [i; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.url == "https://github.com/vmux-ai/vmux/pull/221")
                .count(),
            1
        );
    }

    #[test]
    fn auto_duplicate_url_reuses_pending_page_open_task() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux/pull/221".into(),
                request_id: [0; 16],
                focus: false,
            });
        app.update();

        let first_stack = page_open_requests(&app)
            .iter()
            .find_map(|request| match request.target {
                PageOpenTarget::Stack(stack)
                    if request.url == "https://github.com/vmux-ai/vmux/pull/221" =>
                {
                    Some(stack)
                }
                _ => None,
            })
            .unwrap();
        app.world_mut().spawn(vmux_core::PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack: first_stack,
            url: "https://github.com/vmux-ai/vmux/pull/221".into(),
            request_id: None,
        });

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux/pull/221".into(),
                request_id: [1; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.url == "https://github.com/vmux-ai/vmux/pull/221")
                .count(),
            1
        );
    }

    #[test]
    fn direction_batched_new_type_uses_split_target_size() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://agent/claude/session",
        );

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: Some(PaneDirection::Right),
                url: "file:///repo/crates/vmux_agent/src/plugin.rs".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "vmux://terminal/".into(),
                request_id: [1u8; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let parent_for = |prefix: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url.starts_with(prefix) => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let file_parent = parent_for("file:");
        let terminal_parent = parent_for("vmux://terminal/");
        let file_split = app.world().get::<ChildOf>(file_parent).unwrap().get();

        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            file_split
        );
        assert_eq!(
            app.world().get::<PaneSplit>(file_split).unwrap().direction,
            PaneSplitDirection::Column,
            "the forced-right target is 800x900, so the next pane should split it vertically"
        );
    }

    #[test]
    fn auto_new_type_splits_anchor() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 5, Vec2::new(1600.0, 900.0), "https://a.com");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: browser_pane,
                direction: None,
                url: "file:///x.rs".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        let split = app
            .world()
            .get::<PaneSplit>(browser_pane)
            .expect("a new file type must split the anchor");
        assert_eq!(
            split.direction,
            PaneSplitDirection::Row,
            "wide anchor splits along its longer (x) side => Row"
        );
    }

    #[test]
    fn auto_reuse_focuses_existing_url_without_new_stack() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 5, Vec2::new(800.0, 600.0), "https://a.com");
        let before = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: browser_pane,
                direction: None,
                url: "https://a.com".into(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        let after = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(
            after, before,
            "reuse focuses the existing page; no new stack spawned"
        );
        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "reuse must not split"
        );
    }

    #[test]
    fn auto_reuse_focuses_existing_file_with_different_fragment() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let file_pane = place_pane_with_url(
            &mut app,
            tab,
            5,
            Vec2::new(800.0, 600.0),
            "file:///repo/src/main.rs#L10",
        );
        let before = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "file:///repo/src/main.rs#L42".into(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        let after = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(
            after, before,
            "same file with a new fragment focuses the existing page"
        );
    }

    #[test]
    fn auto_reuse_file_with_different_fragment_navigates_existing_stack() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let file_pane = place_pane_with_url(
            &mut app,
            tab,
            5,
            Vec2::new(800.0, 600.0),
            "file:///repo/src/main.rs#L10",
        );
        let stack = stack_in_pane(&app, file_pane);

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "file:///repo/src/main.rs#L42".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        let opens = page_open_requests(&app);
        assert_eq!(opens.len(), 1);
        match &opens[0] {
            PageOpenRequest {
                target: PageOpenTarget::Stack(target),
                url,
                ..
            } => {
                assert_eq!(*target, stack);
                assert_eq!(url, "file:///repo/src/main.rs#L42");
            }
            other => panic!("expected PageOpenRequest for existing stack, got {other:?}"),
        }
    }

    #[test]
    fn explicit_direction_reuse_focuses_existing_file_with_different_fragment() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let file_pane = place_pane_with_url(
            &mut app,
            tab,
            5,
            Vec2::new(800.0, 600.0),
            "file:///repo/src/main.rs#L10",
        );
        let before = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: Some(PaneDirection::Right),
                url: "file:///repo/src/main.rs#L42".into(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        let after = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(
            after, before,
            "reuse wins before explicit direction can create a duplicate"
        );
        assert!(
            app.world().get::<PaneSplit>(file_pane).is_none(),
            "reuse must not split the existing pane"
        );
    }

    #[test]
    fn reuse_with_focus_false_does_not_activate_existing_tab() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let old_tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt(1),
                ChildOf(space),
            ))
            .id();
        let active_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(10), ChildOf(space)))
            .id();
        let file_pane = place_pane_with_url(
            &mut app,
            old_tab,
            5,
            Vec2::new(800.0, 600.0),
            "file:///repo/src/main.rs#L10",
        );
        place_pane_with_url(
            &mut app,
            active_tab,
            6,
            Vec2::new(800.0, 600.0),
            "https://active.example",
        );

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "file:///repo/src/main.rs#L42".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        assert_eq!(app.world().get::<LastActivatedAt>(old_tab).unwrap().0, 1);
        assert_eq!(
            app.world().get::<LastActivatedAt>(active_tab).unwrap().0,
            10
        );
    }

    #[test]
    fn auto_browser_reuses_bucket_before_terminal_splits_existing_tail() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 600.0), "https://a.com");
        let file_pane =
            place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "vmux://terminal/".into(),
                request_id: [1u8; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let github_parent = parent_for_url("https://github.com/vmux-ai/vmux");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "new browser URL should stack in the existing browser pane"
        );
        assert_eq!(
            github_parent, browser_pane,
            "new browser URL should stack in the existing browser pane"
        );
        assert!(
            app.world().get::<PaneSplit>(file_pane).is_some(),
            "terminal should split the current file tail"
        );
        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            file_pane,
            "terminal should split the current file tail"
        );
    }

    #[test]
    fn auto_browser_reuses_bucket_before_same_batch_terminal_splits_existing_tail() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 600.0), "https://a.com");
        let file_pane =
            place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "vmux://terminal/".into(),
                request_id: [1u8; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let github_parent = parent_for_url("https://github.com/vmux-ai/vmux");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "new browser URL should stack in the existing browser pane"
        );
        assert_eq!(
            github_parent, browser_pane,
            "new browser URL should stack in the existing browser pane"
        );
        assert!(
            app.world().get::<PaneSplit>(file_pane).is_some(),
            "terminal should split the current file tail"
        );
        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            file_pane,
            "terminal should split the current file tail"
        );
    }

    #[test]
    fn forced_split_anchor_keeps_current_tail_when_browser_reuses_bucket() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 600.0), "https://a.com");
        let file_pane =
            place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let github_parent = requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack)
                    if request.url == "https://github.com/vmux-ai/vmux" =>
                {
                    app.world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            github_parent, browser_pane,
            "new browser URL should reuse the existing browser pane"
        );

        app.insert_resource(SplitAnchorInput { anchor: file_pane })
            .init_resource::<SplitAnchorOut>()
            .add_systems(Update, split_anchor_test_sys);
        app.update();

        assert_eq!(app.world().resource::<SplitAnchorOut>().0, Some(file_pane));
    }

    #[test]
    fn forced_split_anchor_ignores_exact_reused_browser_page() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(800.0, 600.0),
            "https://github.com/vmux-ai/vmux",
        );
        let file_pane =
            place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        app.insert_resource(SplitAnchorInput { anchor: file_pane })
            .init_resource::<SplitAnchorOut>()
            .add_systems(Update, split_anchor_test_sys);
        app.update();

        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "exact browser reuse must not split the existing browser pane"
        );
        assert_eq!(app.world().resource::<SplitAnchorOut>().0, Some(file_pane));
    }

    #[derive(Resource)]
    struct SplitAnchorInput {
        anchor: Entity,
    }
    #[derive(Resource, Default)]
    struct SplitAnchorOut(Option<Entity>);

    fn split_anchor_test_sys(
        input: Res<SplitAnchorInput>,
        ctx: PlacementCtx,
        mut out: ResMut<SplitAnchorOut>,
    ) {
        out.0 = Some(resolve_split_anchor_pane(input.anchor, &ctx));
    }

    #[derive(Resource)]
    struct SpiralInput {
        anchor: Entity,
        url: String,
    }
    #[derive(Resource, Default)]
    struct SpiralOut(Option<Entity>);

    fn spiral_test_sys(
        input: Res<SpiralInput>,
        mut commands: Commands,
        ctx: PlacementCtx,
        mut out: ResMut<SpiralOut>,
    ) {
        let mut batch = std::collections::HashSet::new();
        out.0 = Some(resolve_spiral_pane(
            &mut commands,
            input.anchor,
            &input.url,
            false,
            &mut batch,
            &ctx,
        ));
    }

    fn spiral_app(anchor_url: &str, other: Option<(&str, u64, Vec2)>) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SpiralOut>()
            .add_systems(Update, spiral_test_sys);
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent = place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 900.0), anchor_url);
        if let Some((url, seq, size)) = other {
            place_pane_with_url(&mut app, tab, seq, size, url);
        }
        (app, agent)
    }

    #[test]
    fn run_terminal_spirals_off_newest_nonagent_leaf() {
        let (mut app, agent) = spiral_app(
            "vmux://agent/vibe/x",
            Some(("https://a.com", 9, Vec2::new(1600.0, 900.0))),
        );
        let browser = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .find(|&e| e != agent)
            .unwrap();
        app.world_mut().insert_resource(SpiralInput {
            anchor: agent,
            url: "vmux://terminal/".into(),
        });
        app.update();

        let split = app
            .world()
            .get::<PaneSplit>(browser)
            .expect("newest non-agent (browser) leaf must split for the new terminal type");
        assert_eq!(split.direction, PaneSplitDirection::Row, "wide => Row");
        assert!(
            app.world().get::<PaneSplit>(agent).is_none(),
            "agent pane untouched"
        );
        let out = app.world().resource::<SpiralOut>().0.unwrap();
        assert_ne!(out, browser, "returns the new leaf, not the split node");
    }

    #[test]
    fn run_terminal_adds_tab_to_existing_terminal_stack() {
        let (mut app, agent) = spiral_app(
            "vmux://agent/vibe/x",
            Some(("vmux://terminal/7", 9, Vec2::new(1600.0, 900.0))),
        );
        let term_pane = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .find(|&e| e != agent)
            .unwrap();
        app.world_mut().insert_resource(SpiralInput {
            anchor: agent,
            url: "vmux://terminal/".into(),
        });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(term_pane).is_none(),
            "existing terminal stack must not split"
        );
        assert_eq!(
            app.world().resource::<SpiralOut>().0,
            Some(term_pane),
            "new terminal tabs into the existing terminal pane"
        );
    }

    #[test]
    fn force_pane_close_dispatches_pane_close_without_dialog() {
        use vmux_command::CommandPlugin;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin));
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab), ForcePaneClose))
            .id();

        process_force_pane_closes(app.world_mut());

        assert!(
            app.world().get::<ForcePaneClose>(pane).is_none(),
            "ForcePaneClose marker should be consumed"
        );
        assert!(
            app.world().get::<CloseConfirmed>(pane).is_some(),
            "pane should be marked CloseConfirmed so the close skips the dialog"
        );
        let closes: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .filter(|c| {
                matches!(
                    c,
                    AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close))
                )
            })
            .collect();
        assert_eq!(closes.len(), 1, "exactly one PaneCommand::Close dispatched");
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
    fn closing_last_pane_keeps_window_with_fresh_stack() {
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

        assert!(
            !app.world().entity(window).contains::<ClosingWindow>(),
            "closing the last pane must keep the window open"
        );
        let mut panes = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>();
        assert_eq!(
            panes.iter(app.world()).count(),
            1,
            "a fresh leaf pane should replace the closed one"
        );
        let mut stacks = app.world_mut().query_filtered::<Entity, With<Stack>>();
        assert_eq!(
            stacks.iter(app.world()).count(),
            1,
            "the fresh pane should contain one stack"
        );
    }

    #[test]
    fn closing_pane_preserves_surviving_split_direction() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .init_resource::<ConfirmCloseSettings>()
            .add_message::<PageOpenRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_pane_commands.in_set(WriteAppCommands));

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab_e = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                Node {
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                ChildOf(tab_e),
            ))
            .id();
        let left = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        let left_top = place_pane(
            &mut app,
            left,
            Vec2::new(200.0, 200.0),
            Vec2::new(400.0, 400.0),
        );
        let left_bottom = place_pane(
            &mut app,
            left,
            Vec2::new(200.0, 600.0),
            Vec2::new(400.0, 400.0),
        );
        let right = place_pane(
            &mut app,
            root,
            Vec2::new(600.0, 400.0),
            Vec2::new(400.0, 800.0),
        );

        app.world_mut()
            .entity_mut(right)
            .insert(LastActivatedAt::now());

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close)));
        app.update();

        assert!(
            app.world().get_entity(right).is_err(),
            "closed pane should be despawned"
        );
        let split = app
            .world()
            .get::<PaneSplit>(root)
            .expect("root must remain a split after the right pane closes");
        assert_eq!(
            split.direction,
            PaneSplitDirection::Column,
            "surviving left split was horizontal (Column); closing right must keep it Column, not flip to Row"
        );
        let node = app.world().get::<Node>(root).expect("root has a Node");
        assert_eq!(
            node.flex_direction,
            FlexDirection::Column,
            "root Node flex_direction must follow the adopted Column split direction"
        );
        let children = app
            .world()
            .get::<Children>(root)
            .expect("root has children");
        let leaves: Vec<Entity> = children.iter().collect();
        assert_eq!(leaves.len(), 2, "root should hold the two surviving leaves");
        assert!(leaves.contains(&left_top) && leaves.contains(&left_bottom));
    }

    #[test]
    fn closing_one_of_three_siblings_keeps_split_intact() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<NewStackContext>()
            .init_resource::<ConfirmCloseSettings>()
            .add_message::<PageOpenRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_pane_commands.in_set(WriteAppCommands));

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab_e = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                Node {
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                ChildOf(tab_e),
            ))
            .id();
        let a = place_pane(
            &mut app,
            root,
            Vec2::new(200.0, 400.0),
            Vec2::new(400.0, 800.0),
        );
        let b = place_pane(
            &mut app,
            root,
            Vec2::new(600.0, 400.0),
            Vec2::new(400.0, 800.0),
        );
        let c = place_pane(
            &mut app,
            root,
            Vec2::new(1000.0, 400.0),
            Vec2::new(400.0, 800.0),
        );

        app.world_mut().entity_mut(a).insert(LastActivatedAt(10));
        app.world_mut().entity_mut(c).insert(LastActivatedAt(20));
        app.world_mut().entity_mut(b).insert(LastActivatedAt(30));
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close)));
        app.update();

        assert!(
            app.world().get_entity(b).is_err(),
            "the closed middle pane must be despawned"
        );
        let split = app
            .world()
            .get::<PaneSplit>(root)
            .expect("a 3-way split must stay a split after one pane closes");
        assert_eq!(
            split.direction,
            PaneSplitDirection::Row,
            "surviving split keeps its direction"
        );
        let children: Vec<Entity> = app
            .world()
            .get::<Children>(root)
            .expect("root has children")
            .iter()
            .collect();
        assert_eq!(
            children.len(),
            2,
            "root keeps exactly the two surviving leaves directly under it"
        );
        assert!(
            children.contains(&a) && children.contains(&c),
            "both survivors remain direct children of the split"
        );
        assert!(
            app.world().get_entity(a).is_ok() && app.world().get_entity(c).is_ok(),
            "survivors are not despawned"
        );
        for survivor in [a, c] {
            let has_stack = app
                .world()
                .get::<Children>(survivor)
                .is_some_and(|ch| ch.iter().any(|e| app.world().get::<Stack>(e).is_some()));
            assert!(has_stack, "survivor keeps its own stack");
        }
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
    fn pane_hover_activates_hovered_pane_in_single_update() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<crate::scene::InteractionMode>()
            .init_resource::<PaneHoverIntent>()
            .insert_resource(ButtonInput::<KeyCode>::default())
            .add_systems(Update, poll_cursor_pane_focus);

        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.world_mut()
            .entity_mut(window)
            .get_mut::<Window>()
            .unwrap()
            .set_physical_cursor_position(Some(bevy::math::DVec2::new(400.0, 450.0)));
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
        app.world_mut().entity_mut(left).insert(LastActivatedAt(1));
        app.world_mut()
            .entity_mut(right)
            .insert(LastActivatedAt(10));
        let left_stack = app
            .world()
            .get::<Children>(left)
            .unwrap()
            .iter()
            .find(|&e| app.world().get::<Stack>(e).is_some())
            .unwrap();
        app.world_mut()
            .entity_mut(left_stack)
            .insert(LastActivatedAt(1));

        app.update();

        assert!(
            app.world().get::<LastActivatedAt>(left).unwrap().0 > 10,
            "hovered pane should activate in the same update"
        );
        assert!(
            app.world().get::<LastActivatedAt>(left_stack).unwrap().0 > 1,
            "hovered pane active stack should activate in the same update"
        );
    }

    #[test]
    fn pane_hover_uses_native_cursor_position_fallback() {
        let source = include_str!("pane.rs");
        let poll_fn = source
            .split("fn poll_cursor_pane_focus")
            .nth(1)
            .and_then(|tail| tail.split("fn click_pane_in_player_mode").next())
            .unwrap_or_default();

        assert!(poll_fn.contains("pane_hover_cursor_position(window_entity, window)"));
        assert!(source.contains("fn native_window_cursor_position"));
        assert!(source.contains("NSEvent::mouseLocation()"));
        assert!(source.contains("convertPointFromScreen"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn wake_on_move_wakes_without_retargeting_active_pane() {
        super::hover_wake::publish(
            vec![(1, 0.0, 0.0, 100.0, 100.0), (2, 200.0, 0.0, 300.0, 100.0)],
            Some(1),
        );
        assert!(super::wake_on_move(50.0, 50.0));
        assert_eq!(super::hover_wake::take_pending_target(), None);
        assert!(super::wake_on_move(250.0, 50.0));
        assert_eq!(super::hover_wake::take_pending_target(), Some(2));
        assert!(super::wake_on_move(150.0, 50.0));
        assert_eq!(super::hover_wake::take_pending_target(), None);
    }

    #[test]
    fn pane_hover_activates_target_stack() {
        let source = include_str!("pane.rs");
        let poll_fn = source
            .split("fn poll_cursor_pane_focus")
            .nth(1)
            .and_then(|tail| tail.split("fn pane_hover_cursor_position").next())
            .unwrap_or_default();

        assert!(poll_fn.contains("active_stack_in_pane(target"));
        assert!(poll_fn.contains("commands.entity(target_stack).insert(LastActivatedAt::now())"));
    }

    #[test]
    fn pane_hover_runs_before_focus_cache_computes() {
        let source = include_str!("pane.rs");
        let plugin_build = source
            .split("impl Plugin for PanePlugin")
            .nth(1)
            .and_then(|tail| tail.split("fn register_zoom_hooks").next())
            .unwrap_or_default();

        assert!(
            plugin_build.contains("poll_cursor_pane_focus.before(crate::stack::ComputeFocusSet)")
        );
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
    fn split_leaf_into_two_reparents_tabs_and_splits() {
        use bevy_ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let active = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now()))
            .id();
        let existing = app
            .world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(active)))
            .id();

        let p2 = app
            .world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      children: Query<&Children, With<Pane>>,
                      tabq: Query<Entity, With<Stack>>| {
                    let existing_tabs: Vec<Entity> = children
                        .get(active)
                        .map(|c| c.iter().filter(|&e| tabq.contains(e)).collect())
                        .unwrap_or_default();
                    split_leaf_into_two(
                        &mut commands,
                        active,
                        PaneSplitDirection::Row,
                        &existing_tabs,
                        true,
                    )
                },
            )
            .unwrap();

        let world = app.world_mut();
        assert!(
            world.get::<PaneSplit>(active).is_some(),
            "active became a split root"
        );
        assert_ne!(
            world.entity(existing).get::<ChildOf>().unwrap().get(),
            active,
            "stack reparented off active"
        );
        assert!(world.get::<PaneSplit>(p2).is_none(), "p2 is a leaf");
    }

    #[test]
    fn split_or_extend_batched_runs_make_no_empty_panes() {
        use bevy_ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let anchor = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let agent_stack = app
            .world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor)))
            .id();

        // Two agent runs splitting the same anchor in ONE command buffer: the
        // first really splits, the second extends the now-split anchor.
        let (p2a, p2b) = app
            .world_mut()
            .run_system_once(move |mut commands: Commands| {
                let existing = [agent_stack];
                let p2a = split_or_extend(
                    &mut commands,
                    anchor,
                    PaneSplitDirection::Row,
                    &existing,
                    false,
                    false,
                );
                let p2b = split_or_extend(
                    &mut commands,
                    anchor,
                    PaneSplitDirection::Row,
                    &existing,
                    false,
                    true,
                );
                (p2a, p2b)
            })
            .unwrap();

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(anchor)
            .expect("anchor has children")
            .iter()
            .collect();
        assert_eq!(
            children.len(),
            3,
            "anchor holds exactly the stack-holder + two terminal leaves (no orphaned empty pane)"
        );
        assert!(
            children.contains(&p2a) && children.contains(&p2b),
            "both new terminal leaves are direct children of the split"
        );
        let stack_holders = children
            .iter()
            .filter(|&&c| {
                app.world()
                    .get::<Children>(c)
                    .is_some_and(|cc| cc.iter().any(|e| app.world().get::<Stack>(e).is_some()))
            })
            .count();
        assert_eq!(
            stack_holders, 1,
            "the agent stack lives in exactly one child"
        );
        let empty_leaves = children
            .iter()
            .filter(|&&c| {
                app.world()
                    .get::<Children>(c)
                    .map(|cc| cc.iter().count())
                    .unwrap_or(0)
                    == 0
            })
            .count();
        assert_eq!(
            empty_leaves, 2,
            "exactly the two terminal-host leaves are empty; no orphan leftover"
        );
    }

    #[test]
    fn open_beside_splits_the_given_pane() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<SpawnCounter>()
            .add_systems(Update, handle_open_beside_requests);
        let tab = app.world_mut().spawn(crate::tab::tab_bundle()).id();
        let anchor_pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor_pane)));

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: anchor_pane,
                direction: Some(PaneDirection::Right),
                url: "vmux://terminal/".into(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        let world = app.world_mut();
        assert!(world.get::<PaneSplit>(anchor_pane).is_some());
        let kids = world.entity(anchor_pane).get::<Children>().unwrap();
        assert_eq!(kids.iter().count(), 2);
    }

    #[test]
    fn open_beside_with_focus_false_leaves_new_stack_unactivated() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<SpawnCounter>()
            .add_systems(Update, handle_open_beside_requests);
        let tab = app.world_mut().spawn(crate::tab::tab_bundle()).id();
        let anchor_pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor_pane)));

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: anchor_pane,
                direction: Some(PaneDirection::Right),
                url: "vmux://terminal/".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        let world = app.world_mut();
        let mut stacks = world.query_filtered::<&LastActivatedAt, With<Stack>>();
        let unactivated = stacks.iter(world).filter(|la| la.0 == 0).count();
        assert_eq!(
            unactivated, 1,
            "focus:false leaves exactly the new stack un-activated (ts 0) so focus stays put"
        );
    }

    #[test]
    fn batched_open_beside_makes_no_empty_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<SpawnCounter>()
            .add_systems(Update, handle_open_beside_requests);
        let tab = app.world_mut().spawn(crate::tab::tab_bundle()).id();
        let anchor_pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor_pane)));

        // Three open_page calls in one tick, all anchored to the same pane (as
        // the agent does for "open a few terminals beside me").
        for direction in [
            PaneDirection::Right,
            PaneDirection::Bottom,
            PaneDirection::Left,
        ] {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: anchor_pane,
                    direction: Some(direction),
                    url: "vmux://terminal/".into(),
                    request_id: [0u8; 16],
                    focus: false,
                });
        }
        app.update();

        assert!(
            app.world().get::<PaneSplit>(anchor_pane).is_some(),
            "anchor becomes a split"
        );
        let children: Vec<Entity> = app
            .world()
            .get::<Children>(anchor_pane)
            .expect("anchor has children")
            .iter()
            .collect();
        assert_eq!(
            children.len(),
            4,
            "anchor holds the stack-holder + three terminal leaves, with no orphaned empty panes"
        );
        for child in children {
            let has_stack = app
                .world()
                .get::<Children>(child)
                .is_some_and(|cc| cc.iter().any(|e| app.world().get::<Stack>(e).is_some()));
            assert!(
                has_stack,
                "every child pane has a stack; none is an empty orphan"
            );
        }
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
    fn in_pane_new_split_warps_cursor_to_new_pane() {
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

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(pane)
            .unwrap()
            .iter()
            .filter(|e| app.world().get::<Pane>(*e).is_some())
            .collect();

        assert_eq!(
            app.world().resource::<PendingCursorWarp>().target,
            Some(children[1]),
            "split should warp cursor to the newly active pane"
        );
    }

    #[test]
    fn in_pane_new_split_without_url_or_startup_opens_prompt_stack() {
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
                    url: None,
                },
            )));
        app.update();

        assert!(app.world().get::<PaneSplit>(pane).is_some());
        let collected = app.world().resource::<InPaneCollectedSpawns>();
        assert!(collected.0.is_empty());
        let ctx = app.world().resource::<NewStackContext>();
        assert!(ctx.stack.is_some());
        assert!(ctx.needs_open);
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

    #[test]
    fn assign_pane_ids_fills_missing_and_keeps_existing() {
        let mut app = App::new();
        app.add_systems(Update, super::assign_pane_ids);
        let bare = app.world_mut().spawn(super::Pane).id();
        let kept = app
            .world_mut()
            .spawn((super::Pane, super::PaneId("fixed".to_string())))
            .id();
        app.update();
        let assigned = app.world().get::<super::PaneId>(bare).expect("id assigned");
        assert!(!assigned.0.is_empty());
        assert_eq!(app.world().get::<super::PaneId>(kept).unwrap().0, "fixed");
    }
}
