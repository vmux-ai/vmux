use crate::{
    browser::Browser,
    command::{AppCommand, PaneCommand, ReadAppCommands},
    layout::space::Space,
    layout::tab::{Tab, tab_bundle, active_among, active_pane_in_space, active_tab_in_pane,
                  focused_tab},
    settings::AppSettings,
};
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    ui::{FlexDirection, UiGlobalTransform},
    window::PrimaryWindow,
};
use std::time::Instant;
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_history::LastActivatedAt;

pub(crate) struct PanePlugin;

const HOVER_DEBOUNCE_MS: u64 = 80;
const HOVER_COOLDOWN_MS: u64 = 300;

#[derive(Resource, Default)]
pub(crate) struct PaneHoverIntent {
    pub target: Option<Entity>,
    pub since: Option<Instant>,
    pub last_activation: Option<Instant>,
}

impl Plugin for PanePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .add_systems(Update, on_pane_select.in_set(ReadAppCommands))
            .add_systems(Update, handle_pane_commands.in_set(ReadAppCommands))
            .add_systems(Update, poll_cursor_pane_focus)
            .add_systems(Update, pane_gap_drag_resize)
            .add_systems(PostUpdate, warp_cursor_to_active_pane);
    }
}

/// Signals that the cursor should be warped to the active pane once layout is computed.
#[derive(Resource, Default)]
struct PendingCursorWarp {
    target: Option<Entity>,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct Pane;

#[derive(Component, Reflect)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct PaneSplit {
    pub direction: PaneSplitDirection,
}

#[derive(Reflect, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PaneSplitDirection {
    #[default]
    Row,
    Column,
}

#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct PaneSize {
    pub flex_grow: f32,
}

impl Default for PaneSize {
    fn default() -> Self {
        Self { flex_grow: 1.0 }
    }
}

pub(crate) const MIN_PANE_PX: f32 = 60.0;
pub(crate) const RESIZE_STEP: f32 = 0.05;

/// Temporary component inserted on a PaneSplit entity while the user is
/// dragging the gap between two of its children.
#[derive(Component)]
pub(crate) struct PaneDrag {
    prev_child: Entity,
    next_child: Entity,
    start_pos: f32,
    start_prev_grow: f32,
    start_next_grow: f32,
}

pub(crate) fn leaf_pane_bundle() -> impl Bundle {
    (
        Pane::default(),
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

fn spawn_leaf_pane(commands: &mut Commands, parent: Entity) -> Entity {
    commands.spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(parent))).id()
}

/// Compute clamped flex_grow values after a resize delta.
/// Returns (new_pane_grow, new_sibling_grow).
fn compute_resize(
    pane_grow: f32,
    sib_grow: f32,
    delta: f32,
    parent_len: f32,
) -> (f32, f32) {
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

pub(crate) fn first_leaf_descendant(
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

pub(crate) fn first_tab_in_pane(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    tab_q: &Query<Entity, With<Tab>>,
) -> Option<Entity> {
    let children = pane_children.get(pane).ok()?;
    children.iter().find(|&e| tab_q.contains(e))
}

fn handle_pane_commands(
    mut reader: MessageReader<AppCommand>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of_q: Query<&ChildOf>,
    split_dir_q: Query<&PaneSplit>,
    tab_filter: Query<Entity, With<Tab>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    mut resize_q: ParamSet<(Query<&mut Node>, Query<&mut PaneSize>, Query<&ComputedNode>, ResMut<PendingCursorWarp>)>,
) {
    for cmd in reader.read() {
        let AppCommand::Pane(pane_cmd) = *cmd else {
            continue;
        };
        let (_, active_pane_opt, _) = focused_tab(
            &spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts,
        );
        let Some(active) = active_pane_opt else {
            continue;
        };

        match pane_cmd {
            PaneCommand::SplitV | PaneCommand::SplitH => {
                let direction = if pane_cmd == PaneCommand::SplitV {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                };

                let existing_tabs: Vec<Entity> = pane_children
                    .get(active)
                    .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                    .unwrap_or_default();

                let pane1 = spawn_leaf_pane(&mut commands, active);
                let pane2 = spawn_leaf_pane(&mut commands, active);

                for tab in existing_tabs {
                    commands.entity(tab).insert(ChildOf(pane1));
                }

                let startup_url = settings.browser.startup_url.as_str();
                let new_tab = commands.spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane2))).id();
                commands.spawn((
                    Browser::new(&mut meshes, &mut webview_mt, startup_url),
                    ChildOf(new_tab),
                ));

                let split_dir = if pane_cmd == PaneCommand::SplitV {
                    PaneSplitDirection::Row
                } else {
                    PaneSplitDirection::Column
                };
                commands.entity(active).insert(PaneSplit { direction: split_dir });
                let gap = Val::Px(settings.layout.pane.gap);
                commands.entity(active).insert(Node {
                    flex_grow: 1.0,
                    flex_direction: direction,
                    column_gap: gap,
                    row_gap: gap,
                    align_items: AlignItems::Stretch,
                    ..default()
                });

                commands.entity(pane2).insert(LastActivatedAt::now());
                hover_intent.target = None;
                hover_intent.last_activation = Some(Instant::now());
                resize_q.p3().target = Some(pane2);
            }
            PaneCommand::Close => {
                let Ok(pane_co) = child_of_q.get(active) else {
                    continue;
                };
                let parent = pane_co.get();

                if !split_dir_q.contains(parent) {
                    commands.entity(active).despawn();
                    let startup_url = settings.browser.startup_url.as_str();
                    let leaf = spawn_leaf_pane(&mut commands, parent);
                    let tab = commands.spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(leaf))).id();
                    commands.spawn((
                        Browser::new(&mut meshes, &mut webview_mt, startup_url),
                        ChildOf(tab),
                    ));
                    commands.entity(leaf).insert(LastActivatedAt::now());
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
                commands.entity(new_active_pane).insert(LastActivatedAt::now());
                let tab = active_tab_in_pane(new_active_pane, &pane_children, &tab_ts)
                    .or_else(|| first_tab_in_pane(new_active_pane, &pane_children, &tab_filter))
                    .or_else(|| sibling_children.iter().copied().find(|&e| tab_filter.contains(e)));
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
            PaneCommand::SwapPrev => {}
            PaneCommand::SwapNext => {}
            PaneCommand::RotateForward => {}
            PaneCommand::RotateBackward => {}
            PaneCommand::EqualizeSize => {
                let Ok(co) = child_of_q.get(active) else { continue };
                let parent = co.get();
                if !split_dir_q.contains(parent) { continue; }
                let Ok(children) = all_children.get(parent) else { continue };
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
            PaneCommand::ResizeLeft | PaneCommand::ResizeRight
            | PaneCommand::ResizeUp | PaneCommand::ResizeDown => {
                let target_axis = match pane_cmd {
                    PaneCommand::ResizeLeft | PaneCommand::ResizeRight => PaneSplitDirection::Row,
                    _ => PaneSplitDirection::Column,
                };
                let grows = matches!(
                    pane_cmd,
                    PaneCommand::ResizeRight | PaneCommand::ResizeDown
                );

                let mut child_in_split = active;
                let mut found_parent: Option<Entity> = None;
                for _ in 0..10 {
                    let Ok(co) = child_of_q.get(child_in_split) else { break };
                    let parent = co.get();
                    if let Ok(ps) = split_dir_q.get(parent) {
                        if ps.direction == target_axis {
                            found_parent = Some(parent);
                            break;
                        }
                    }
                    child_in_split = parent;
                }
                let Some(parent) = found_parent else { continue };
                let Ok(siblings) = all_children.get(parent) else { continue };
                let sibs: Vec<Entity> = siblings.iter().collect();
                let Some(idx) = sibs.iter().position(|&e| e == child_in_split) else { continue };

                let (pane_entity, sibling_entity) = if grows {
                    if idx + 1 >= sibs.len() { continue; }
                    (child_in_split, sibs[idx + 1])
                } else {
                    if idx == 0 { continue; }
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
                    if let Ok(mut n) = nq.get_mut(pane_entity) { n.flex_grow = pg; }
                    if let Ok(mut n) = nq.get_mut(sibling_entity) { n.flex_grow = sg; }
                }
                {
                    let mut sq = resize_q.p1();
                    if let Ok(mut ps) = sq.get_mut(pane_entity) { ps.flex_grow = pg; }
                    if let Ok(mut ps) = sq.get_mut(sibling_entity) { ps.flex_grow = sg; }
                }
            }
        }
    }
}

fn on_pane_select(
    mut reader: MessageReader<AppCommand>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_pos_q: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    mut pending_warp: ResMut<PendingCursorWarp>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let direction: Vec2 = match cmd {
            AppCommand::Pane(PaneCommand::SelectLeft) => Vec2::new(-1.0, 0.0),
            AppCommand::Pane(PaneCommand::SelectRight) => Vec2::new(1.0, 0.0),
            AppCommand::Pane(PaneCommand::SelectUp) => Vec2::new(0.0, -1.0),
            AppCommand::Pane(PaneCommand::SelectDown) => Vec2::new(0.0, 1.0),
            _ => continue,
        };
        let active_space = active_among(spaces.iter());
        let Some(space) = active_space else {
            continue;
        };
        let panes = collect_space_leaf_panes(space, &all_children, &leaf_pane_q);
        if panes.len() < 2 {
            continue;
        }
        let current = active_pane_in_space(space, &all_children, &leaf_pane_q, &pane_ts);
        let Some(current) = current else {
            continue;
        };
        let Ok((cur_node, cur_gt)) = pane_pos_q.get(current) else {
            continue;
        };
        let cur_center = cur_gt.transform_point2(Vec2::ZERO);
        let cur_size = cur_node.size;

        let mut best: Option<(Entity, f32)> = None;
        for &pane in &panes {
            if pane == current {
                continue;
            }
            let Ok((_, gt)) = pane_pos_q.get(pane) else {
                continue;
            };
            let center = gt.transform_point2(Vec2::ZERO);
            let delta = center - cur_center;

            let along = delta.dot(direction);
            if along <= 0.0 {
                continue;
            }

            let cross_axis = Vec2::new(-direction.y, direction.x);
            let cross = delta.dot(cross_axis).abs();
            let threshold = if direction.x.abs() > 0.5 {
                cur_size.y * 0.5
            } else {
                cur_size.x * 0.5
            };
            if cross > threshold {
                continue;
            }

            let dist = delta.length();
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((pane, dist));
            }
        }

        if let Some((target, _)) = best {
            hover_intent.target = None;
            hover_intent.last_activation = Some(Instant::now());
            commands.entity(target).insert(LastActivatedAt::now());
            pending_warp.target = Some(target);
        }
    }
}

fn poll_cursor_pane_focus(
    windows: Query<&Window, With<PrimaryWindow>>,
    leaf_panes: Query<(Entity, &ComputedNode, &UiGlobalTransform), (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    mut intent: ResMut<PaneHoverIntent>,
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    active_drags: Query<(), With<PaneDrag>>,
) {
    if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
        return;
    }
    if !active_drags.is_empty() {
        return;
    }
    if let Some(last) = intent.last_activation {
        if last.elapsed().as_millis() < HOVER_COOLDOWN_MS as u128 {
            return;
        }
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
        leaf_panes.iter().filter_map(|(e, _, _)| pane_ts.get(e).ok()),
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
    let Some(cursor_pos) = window.physical_cursor_position() else { return };
    let cursor = Vec2::new(cursor_pos.x as f32, cursor_pos.y as f32);

    // --- Handle active drag ---
    if let Ok((split_entity, drag, split)) = active_drags.single() {
        if mouse.pressed(MouseButton::Left) {
            let pos_along = match split.direction {
                PaneSplitDirection::Row => cursor.x,
                PaneSplitDirection::Column => cursor.y,
            };
            let parent_size = parent_nodes.get(split_entity)
                .map(|cn| cn.size).unwrap_or(Vec2::ONE);
            let parent_len = match split.direction {
                PaneSplitDirection::Row => parent_size.x,
                PaneSplitDirection::Column => parent_size.y,
            }.max(1.0);

            let (pg, sg) = compute_resize(
                drag.start_prev_grow,
                drag.start_next_grow,
                (pos_along - drag.start_pos) / parent_len * (drag.start_prev_grow + drag.start_next_grow),
                parent_len,
            );

            if let Ok(mut n) = node_q.get_mut(drag.prev_child) { n.flex_grow = pg; }
            if let Ok(mut n) = node_q.get_mut(drag.next_child) { n.flex_grow = sg; }
            if let Ok(mut s) = size_q.get_mut(drag.prev_child) { s.flex_grow = pg; }
            if let Ok(mut s) = size_q.get_mut(drag.next_child) { s.flex_grow = sg; }
        } else {
            commands.entity(split_entity).remove::<PaneDrag>();
        }

        return;
    }

    // --- Hover detection + drag initiation ---
    'outer: for (split_entity, split, children) in &splits {
        let sibs: Vec<Entity> = children.iter().collect();
        for i in 0..sibs.len().saturating_sub(1) {
            let Ok((node_a, gt_a)) = child_nodes.get(sibs[i]) else { continue };
            let Ok((node_b, gt_b)) = child_nodes.get(sibs[i + 1]) else { continue };

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

            if pos_along >= gap_min && pos_along <= gap_max
                && pos_cross >= cross_min && pos_cross <= cross_max
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

fn collect_space_leaf_panes(
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
