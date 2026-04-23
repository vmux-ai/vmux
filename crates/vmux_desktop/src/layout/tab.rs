use crate::{
    browser::Browser,
    command::{AppCommand, ReadAppCommands, TabCommand, TerminalCommand},
    command_bar::NewTabContext,
    layout::pane::{first_leaf_descendant, first_tab_in_pane, Pane, PaneSplit},
    layout::space::Space,
    settings::AppSettings,
    terminal::Terminal,
};
use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_history::LastActivatedAt;
use vmux_terminal::event::TERMINAL_WEBVIEW_URL;

pub(crate) struct TabPlugin;

impl Plugin for TabPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Tab>()
            .add_systems(Update, handle_tab_commands.in_set(ReadAppCommands))
            .add_systems(PostUpdate, sync_tab_picking);
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct Tab {
    pub scroll_x: f32,
    pub scroll_y: f32,
}

/// Returns the entity with the highest `LastActivatedAt` timestamp.
pub(crate) fn active_among<'a>(
    entities: impl Iterator<Item = (Entity, &'a LastActivatedAt)>,
) -> Option<Entity> {
    entities.max_by_key(|(_, ts)| ts.0).map(|(e, _)| e)
}

/// Recursively collects leaf panes (panes without PaneSplit) under `root`.
pub(crate) fn collect_leaf_panes(
    root: Entity,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    result: &mut Vec<Entity>,
) {
    if leaf_panes.contains(root) {
        result.push(root);
    }
    if let Ok(children) = all_children.get(root) {
        for child in children.iter() {
            collect_leaf_panes(child, all_children, leaf_panes, result);
        }
    }
}

/// Find the active pane (max LastActivatedAt) among leaf panes under a space.
pub(crate) fn active_pane_in_space(
    space: Entity,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: &Query<(Entity, &LastActivatedAt), With<Pane>>,
) -> Option<Entity> {
    let mut panes = Vec::new();
    collect_leaf_panes(space, all_children, leaf_panes, &mut panes);
    active_among(panes.iter().filter_map(|&e| pane_ts.get(e).ok()))
}

/// Find the active tab (max LastActivatedAt) in a pane.
pub(crate) fn active_tab_in_pane(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    tab_ts: &Query<(Entity, &LastActivatedAt), With<Tab>>,
) -> Option<Entity> {
    pane_children.get(pane).ok().and_then(|children| {
        active_among(children.iter().filter_map(|e| tab_ts.get(e).ok()))
    })
}

/// Find the globally focused (space, pane, tab) by chaining `active_among()`.
pub(crate) fn focused_tab(
    spaces: &Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: &Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: &Query<&Children, With<Pane>>,
    tab_ts: &Query<(Entity, &LastActivatedAt), With<Tab>>,
) -> (Option<Entity>, Option<Entity>, Option<Entity>) {
    let space = active_among(spaces.iter());
    let pane = space.and_then(|s| active_pane_in_space(s, all_children, leaf_panes, pane_ts));
    let tab = pane.and_then(|p| active_tab_in_pane(p, pane_children, tab_ts));
    (space, pane, tab)
}

pub(crate) fn tab_bundle() -> impl Bundle {
    (
        Tab::default(),
        vmux_header::PageMetadata::default(),
        Transform::default(),
        GlobalTransform::default(),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
        ZIndex(0),
    )
}

fn handle_tab_commands(
    mut reader: MessageReader<AppCommand>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    tab_q: Query<Entity, With<Tab>>,
    child_of_q: Query<&ChildOf>,
    split_dir_q: Query<&PaneSplit>,
    settings: Res<AppSettings>,
    mut new_tab_ctx: ResMut<NewTabContext>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for cmd in reader.read() {
        let (tab_cmd, is_terminal) = match *cmd {
            AppCommand::Tab(t) => (t, false),
            AppCommand::Terminal(TerminalCommand::New) => (TabCommand::New, true),
            _ => continue,
        };

        let (_, active_pane, active_tab) = focused_tab(
            &spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts,
        );

        match tab_cmd {
            TabCommand::New => {
                let Some(pane) = active_pane else {
                    continue;
                };
                if is_terminal {
                    let tab = commands
                        .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id();
                    commands.entity(tab).insert(vmux_header::PageMetadata {
                        url: TERMINAL_WEBVIEW_URL.to_string(),
                        title: "Terminal (Session: -)".to_string(),
                        ..default()
                    });
                    commands.spawn((
                        Terminal::new(&mut meshes, &mut webview_mt, &settings),
                        ChildOf(tab),
                    ));
                } else {
                    // If there's already an empty tab pending, reuse it
                    if new_tab_ctx.tab.is_some() {
                        new_tab_ctx.needs_open = true;
                        continue;
                    }
                    let tab = commands
                        .spawn((
                            tab_bundle(),
                            LastActivatedAt::now(),
                            ChildOf(pane),
                            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.85)),
                        ))
                        .id();
                    new_tab_ctx.tab = Some(tab);
                    new_tab_ctx.previous_tab = active_tab;
                    new_tab_ctx.needs_open = true;
                }
            }
            TabCommand::Close => {
                let Some(pane) = active_pane else {
                    continue;
                };
                let Some(active) = active_tab else {
                    continue;
                };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let tabs_in_pane: Vec<Entity> = children
                    .iter()
                    .filter(|&e| tab_q.contains(e))
                    .collect();
                if tabs_in_pane.len() <= 1 {
                    let Ok(pane_co) = child_of_q.get(pane) else {
                        continue;
                    };
                    let parent = pane_co.get();

                    if !split_dir_q.contains(parent) {
                        commands.entity(active).despawn();
                        continue;
                    }

                    commands.entity(active).despawn();

                    let Ok(siblings) = pane_children.get(parent) else {
                        continue;
                    };
                    let sibling = siblings
                        .iter()
                        .find(|&e| e != pane && (leaf_panes.contains(e) || split_dir_q.contains(e)));
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

                    commands.entity(pane).despawn();
                    commands.entity(new_active_pane).insert(LastActivatedAt::now());
                    let new_tab = active_tab_in_pane(new_active_pane, &pane_children, &tab_ts)
                        .or_else(|| first_tab_in_pane(new_active_pane, &pane_children, &tab_q))
                        .or_else(|| sibling_children.iter().copied().find(|&e| tab_q.contains(e)));
                    if let Some(t) = new_tab {
                        commands.entity(t).insert(LastActivatedAt::now());
                    }
                    continue;
                }
                let next = active_among(
                    tabs_in_pane
                        .iter()
                        .filter(|&&e| e != active)
                        .filter_map(|&e| tab_ts.get(e).ok()),
                )
                .unwrap();
                commands.entity(active).despawn();
                commands.entity(next).insert(LastActivatedAt::now());
            }
            TabCommand::Next | TabCommand::Previous => {
                let Some(pane) = active_pane else {
                    continue;
                };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let tabs: Vec<Entity> = children
                    .iter()
                    .filter(|&e| tab_q.contains(e))
                    .collect();
                if tabs.len() < 2 {
                    continue;
                }
                let Some(current) = tabs.iter().position(|&e| Some(e) == active_tab) else {
                    continue;
                };
                let delta: i32 = if tab_cmd == TabCommand::Next { 1 } else { -1 };
                let n = tabs.len() as i32;
                let idx = (current as i32 + delta).rem_euclid(n) as usize;
                commands.entity(tabs[idx]).insert(LastActivatedAt::now());
            }
            TabCommand::SelectIndex1
            | TabCommand::SelectIndex2
            | TabCommand::SelectIndex3
            | TabCommand::SelectIndex4
            | TabCommand::SelectIndex5
            | TabCommand::SelectIndex6
            | TabCommand::SelectIndex7
            | TabCommand::SelectIndex8
            | TabCommand::SelectLast => {
                let Some(pane) = active_pane else {
                    continue;
                };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let tabs: Vec<Entity> = children
                    .iter()
                    .filter(|&e| tab_q.contains(e))
                    .collect();
                if tabs.is_empty() {
                    continue;
                }
                let target_idx = match tab_cmd {
                    TabCommand::SelectIndex1 => 0,
                    TabCommand::SelectIndex2 => 1,
                    TabCommand::SelectIndex3 => 2,
                    TabCommand::SelectIndex4 => 3,
                    TabCommand::SelectIndex5 => 4,
                    TabCommand::SelectIndex6 => 5,
                    TabCommand::SelectIndex7 => 6,
                    TabCommand::SelectIndex8 => 7,
                    TabCommand::SelectLast => tabs.len() - 1,
                    _ => continue,
                };
                if target_idx >= tabs.len() {
                    continue;
                }
                commands.entity(tabs[target_idx]).insert(LastActivatedAt::now());
            }
            TabCommand::Reopen
            | TabCommand::Duplicate
            | TabCommand::Pin
            | TabCommand::Mute
            | TabCommand::MoveToPane => {}
        }
    }
}

fn sync_tab_picking(
    pane_children: Query<&Children, With<Pane>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    mut tabs: Query<(Entity, &mut ZIndex), With<Tab>>,
) {
    for pane in &leaf_panes {
        let active = active_tab_in_pane(pane, &pane_children, &tab_ts);
        if let Ok(children) = pane_children.get(pane) {
            for child in children.iter() {
                if let Ok((entity, mut z)) = tabs.get_mut(child) {
                    let target = if Some(entity) == active { ZIndex(1) } else { ZIndex(0) };
                    if *z != target {
                        *z = target;
                    }
                }
            }
        }
    }
}
