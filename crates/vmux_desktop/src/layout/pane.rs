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

pub(crate) fn leaf_pane_bundle() -> impl Bundle {
    (
        Pane::default(),
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
    split_q: Query<(), With<PaneSplit>>,
    tab_filter: Query<Entity, With<Tab>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    mut pending_warp: ResMut<PendingCursorWarp>,
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
                pending_warp.target = Some(pane2);
            }
            PaneCommand::Close => {
                let Ok(pane_co) = child_of_q.get(active) else {
                    continue;
                };
                let parent = pane_co.get();

                if !split_q.contains(parent) {
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
                    .find(|&e| e != active && (leaf_panes.contains(e) || split_q.contains(e)));
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
                if split_q.contains(sibling) {
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
            PaneCommand::EqualizeSize => {}
            PaneCommand::ResizeLeft => {}
            PaneCommand::ResizeRight => {}
            PaneCommand::ResizeUp => {}
            PaneCommand::ResizeDown => {}
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
        }
    }
}

fn poll_cursor_pane_focus(
    windows: Query<&Window, With<PrimaryWindow>>,
    leaf_panes: Query<(Entity, &ComputedNode, &UiGlobalTransform), (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    mut intent: ResMut<PaneHoverIntent>,
    mut commands: Commands,
) {
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
