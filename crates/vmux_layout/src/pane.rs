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
#[cfg(feature = "player-mode")]
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::{
    ecs::{
        lifecycle::HookContext, message::Messages, relationship::Relationship, world::DeferredWorld,
    },
    prelude::*,
    ui::{FlexDirection, UiGlobalTransform},
    window::PrimaryWindow,
};
#[cfg(feature = "player-mode")]
use bevy_cef::prelude::CefKeyboardTarget;
use moonshine_save::prelude::*;
use std::time::Instant;
use vmux_command::{
    AppCommand, BrowserCommand, LayoutCommand, OpenCommand, PaneCommand, ReadAppCommands,
    open::{PaneDirection, PaneOpenMode, PaneTarget},
};
use vmux_core::{PageOpenRequest, PageOpenTarget, PageOpenTask};
use vmux_history::LastActivatedAt;

pub struct PanePlugin;

impl Plugin for PanePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Pane>()
            .register_type::<PaneId>()
            .register_type::<SideSheetCardCollapsed>()
            .register_type::<PaneSplit>()
            .register_type::<PaneSplitDirection>()
            .register_type::<PaneSize>()
            .register_type::<SpawnSeq>()
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<SpawnCounter>()
            .add_systems(Update, repair_stacks_parented_to_splits)
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
        #[cfg(feature = "player-mode")]
        app.add_systems(Update, click_pane_in_player_mode);
        #[cfg(target_os = "macos")]
        app.add_systems(
            Update,
            apply_pending_hover.before(crate::stack::ComputeFocusSet),
        );
        #[cfg(not(target_os = "macos"))]
        app.add_systems(
            Update,
            poll_cursor_pane_focus.before(crate::stack::ComputeFocusSet),
        );
        register_zoom_hooks(app);
    }
}

/// Marker: pane is waiting for close confirmation dialog.
#[derive(Component)]
pub struct PendingPaneClose;

/// Marker: close this pane immediately, without a confirmation dialog. Used when
/// the pane's process has already exited (e.g. an agent CLI quit), so there is
/// nothing to confirm and the pane should be removed + the split collapsed.
#[derive(Component)]
pub struct ForcePaneClose;

#[cfg_attr(target_os = "macos", allow(dead_code))]
const HOVER_COOLDOWN_MS: u64 = 300;

#[derive(Resource, Default)]
pub struct PaneHoverIntent {
    pub target: Option<Entity>,
    pub last_activation: Option<Instant>,
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

#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct SideSheetCardCollapsed;

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

fn repair_stacks_parented_to_splits(
    splits: Query<
        (Entity, &Children),
        (With<PaneSplit>, Or<(Added<PaneSplit>, Changed<Children>)>),
    >,
    pane_children: Query<&Children, With<Pane>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<(), With<Stack>>,
    mut commands: Commands,
) {
    for (split, children) in &splits {
        let direct_stacks: Vec<Entity> = children
            .iter()
            .filter(|&child| stacks.contains(child))
            .collect();
        if direct_stacks.is_empty() {
            continue;
        }
        let mut leaf = first_leaf_descendant(split, &pane_children, &leaf_panes);
        if leaf == split {
            leaf = commands
                .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(split)))
                .id();
        }
        warn!(
            "Repairing {} stack(s) parented directly to pane split {:?}",
            direct_stacks.len(),
            split
        );
        for stack in direct_stacks {
            commands.entity(stack).insert(ChildOf(leaf));
        }
    }
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
        (!req.url.starts_with("file:") && !req.url.starts_with("vmux://"))
            .then_some(req.request_id),
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

        let resolved = vmux_command::open::resolve_url(
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
                None,
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
                            None,
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
                        None,
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
    request_id: Option<[u8; 16]>,
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
            request_id,
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
fn apply_pending_hover(
    mode: Res<crate::scene::InteractionMode>,
    leaf_panes: Query<
        (Entity, &ComputedNode, &UiGlobalTransform),
        (With<Pane>, Without<PaneSplit>),
    >,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
    mut last_motion_sequence: Local<u64>,
) {
    let Some(pointer) = crate::native_pointer::snapshot() else {
        return;
    };
    if pointer.motion_sequence == 0 || pointer.motion_sequence == *last_motion_sequence {
        return;
    }
    *last_motion_sequence = pointer.motion_sequence;
    if *mode != crate::scene::InteractionMode::User {
        return;
    }
    let target = leaf_panes.iter().find_map(|(entity, node, ui_gt)| {
        let center = ui_gt.transform_point2(Vec2::ZERO);
        let half = node.size * 0.5;
        let min = center - half;
        let max = center + half;
        (pointer.position_px.x >= min.x
            && pointer.position_px.x <= max.x
            && pointer.position_px.y >= min.y
            && pointer.position_px.y <= max.y)
            .then_some(entity)
    });
    let Some(target) = target else {
        return;
    };
    let current = active_among(
        leaf_panes
            .iter()
            .filter_map(|(entity, _, _)| pane_ts.get(entity).ok()),
    );
    if current == Some(target) {
        return;
    }
    commands.entity(target).insert(LastActivatedAt::now());
    if let Some(stack) = active_stack_in_pane(target, &pane_children, &stack_ts) {
        commands.entity(stack).insert(LastActivatedAt::now());
    }
}

#[cfg(feature = "player-mode")]
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
#[path = "pane.test.rs"]
mod tests;
