use crate::{
    command::{AppCommand, ReadAppCommands, ServiceCommand, TabCommand, TerminalCommand},
    command_bar::NewTabContext,
    layout::pane::{Pane, PaneSplit, PendingCursorWarp, first_leaf_descendant, first_tab_in_pane},
    layout::space::Space,
    layout::swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
    processes_monitor::ProcessesMonitor,
    settings::AppSettings,
    terminal::Terminal,
};
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    window::{ClosingWindow, PrimaryWindow},
};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_history::LastActivatedAt;
use vmux_terminal::event::TERMINAL_WEBVIEW_URL;

/// Cached result of `focused_tab()`, computed once per frame in `Update`
/// after all command handlers. Read by push/sync systems to avoid redundant
/// tree walks.
#[derive(Resource, Default)]
pub(crate) struct FocusedTab {
    pub space: Option<Entity>,
    pub pane: Option<Entity>,
    pub tab: Option<Entity>,
}

/// System set for `compute_focused_tab`. Systems that read `Res<FocusedTab>`
/// should be ordered `.after(ComputeFocusSet)` in `Update`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ComputeFocusSet;

/// Marker: tab is waiting for close confirmation dialog.
#[derive(Component)]
pub(crate) struct PendingTabClose;

/// Marker: close was confirmed, skip dialog next time.
#[derive(Component)]
pub(crate) struct CloseConfirmed;

/// System set for `handle_tab_commands`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TabCommandSet;

pub(crate) struct TabPlugin;

impl Plugin for TabPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Tab>()
            .init_resource::<FocusedTab>()
            .add_systems(
                Update,
                handle_tab_commands
                    .in_set(ReadAppCommands)
                    .in_set(TabCommandSet),
            )
            .add_systems(
                Update,
                compute_focused_tab
                    .in_set(ComputeFocusSet)
                    .after(ReadAppCommands),
            )
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
    pane_children
        .get(pane)
        .ok()
        .and_then(|children| active_among(children.iter().filter_map(|e| tab_ts.get(e).ok())))
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

fn compute_focused_tab(
    mut cached: ResMut<FocusedTab>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
) {
    let (space, pane, tab) = focused_tab(
        &spaces,
        &all_children,
        &leaf_panes,
        &pane_ts,
        &pane_children,
        &tab_ts,
    );
    cached.space = space;
    cached.pane = pane;
    cached.tab = tab;
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
    mut pending_cursor_warp: ResMut<PendingCursorWarp>,
) {
    for cmd in reader.read() {
        let (tab_cmd, is_terminal, is_processes) = match *cmd {
            AppCommand::Tab(t) => (t, false, false),
            AppCommand::Terminal(TerminalCommand::New) => (TabCommand::New, true, false),
            AppCommand::Service(ServiceCommand::Open) => (TabCommand::New, false, true),
            _ => continue,
        };

        let (active_space, active_pane, active_tab) = focused_tab(
            &spaces,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &tab_ts,
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
                        title: "Terminal".to_string(),
                        ..default()
                    });
                    commands.spawn((
                        Terminal::new(&mut meshes, &mut webview_mt, &settings),
                        ChildOf(tab),
                    ));
                } else if is_processes {
                    let tab = commands
                        .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id();
                    commands.entity(tab).insert(vmux_header::PageMetadata {
                        url: vmux_processes::event::PROCESSES_WEBVIEW_URL.to_string(),
                        title: "Background Services".to_string(),
                        ..default()
                    });
                    commands.spawn((
                        ProcessesMonitor::new(&mut meshes, &mut webview_mt),
                        ChildOf(tab),
                    ));
                } else {
                    // If there's already an empty tab pending, reuse it
                    if new_tab_ctx.tab.is_some() {
                        new_tab_ctx.needs_open = true;
                        continue;
                    }
                    let tab = commands
                        .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
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
                let tabs_in_pane: Vec<Entity> =
                    children.iter().filter(|&e| tab_q.contains(e)).collect();
                if tabs_in_pane.len() <= 1 {
                    if let Some(space) = active_space
                        && close_space_if_only_closing_tab(
                            space,
                            active,
                            &spaces,
                            &child_of_q,
                            &all_children,
                            &tab_q,
                            &mut commands,
                        )
                    {
                        if new_tab_ctx.tab == Some(active) {
                            new_tab_ctx.tab = None;
                        }
                        new_tab_ctx.previous_tab = None;
                        new_tab_ctx.needs_open = false;
                        continue;
                    }

                    if leaf_panes.iter().count() <= 1 {
                        commands.entity(active).despawn();
                        let tab = commands
                            .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                            .id();
                        new_tab_ctx.tab = Some(tab);
                        new_tab_ctx.previous_tab = None;
                        new_tab_ctx.needs_open = true;
                        continue;
                    }

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
                    let sibling = siblings.iter().find(|&e| {
                        e != pane && (leaf_panes.contains(e) || split_dir_q.contains(e))
                    });
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
                        new_active_pane =
                            first_leaf_descendant(sibling, &pane_children, &leaf_panes);
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
                    commands
                        .entity(new_active_pane)
                        .insert(LastActivatedAt::now());
                    let new_tab = active_tab_in_pane(new_active_pane, &pane_children, &tab_ts)
                        .or_else(|| first_tab_in_pane(new_active_pane, &pane_children, &tab_q))
                        .or_else(|| {
                            sibling_children
                                .iter()
                                .copied()
                                .find(|&e| tab_q.contains(e))
                        });
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
                // If an empty tab is pending (Cmd+T command bar), despawn it
                // and exclude from navigation targets.
                let empty_tab = new_tab_ctx.tab.take();
                let prev_tab = new_tab_ctx.previous_tab.take();
                if let Some(e) = empty_tab {
                    commands.entity(e).despawn();
                }

                let Some(space) = active_among(spaces.iter()) else {
                    continue;
                };
                // Build flat list of (pane_entity, tab_entity) across all panes
                let mut space_panes = Vec::new();
                collect_leaf_panes(space, &all_children, &leaf_panes, &mut space_panes);
                let mut flat: Vec<(Entity, Entity)> = Vec::new();
                for &pane_e in &space_panes {
                    if let Ok(children) = pane_children.get(pane_e) {
                        for child in children.iter() {
                            if tab_q.contains(child) && Some(child) != empty_tab {
                                flat.push((pane_e, child));
                            }
                        }
                    }
                }
                if flat.len() < 2 {
                    continue;
                }
                // When coming from an empty tab, use previous_tab as current
                // position; otherwise use active_tab.
                let effective_current = if empty_tab.is_some() {
                    prev_tab.or(active_tab)
                } else {
                    active_tab
                };
                let Some(current) = flat.iter().position(|&(_, t)| Some(t) == effective_current)
                else {
                    continue;
                };
                let delta: i32 = if tab_cmd == TabCommand::Next { 1 } else { -1 };
                let n = flat.len() as i32;
                let idx = (current as i32 + delta).rem_euclid(n) as usize;
                let (target_pane, target_tab) = flat[idx];
                commands.entity(target_tab).insert(LastActivatedAt::now());
                if active_pane != Some(target_pane) {
                    commands.entity(target_pane).insert(LastActivatedAt::now());
                    pending_cursor_warp.target = Some(target_pane);
                }
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
                let empty_tab = new_tab_ctx.tab;

                let Some(pane) = active_pane else {
                    continue;
                };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let tabs: Vec<Entity> = children
                    .iter()
                    .filter(|&e| tab_q.contains(e) && Some(e) != empty_tab)
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
                    // Target doesn't exist — ignore (keep command bar open)
                    continue;
                }

                // Target is valid — despawn empty tab and request modal dismiss
                if let Some(e) = new_tab_ctx.tab.take() {
                    commands.entity(e).despawn();
                    new_tab_ctx.previous_tab = None;
                    new_tab_ctx.dismiss_modal = true;
                }

                commands
                    .entity(tabs[target_idx])
                    .insert(LastActivatedAt::now());
            }
            TabCommand::Reopen | TabCommand::Duplicate | TabCommand::MoveToPane => {}
            TabCommand::SwapPrev | TabCommand::SwapNext => {
                let Some(pane) = active_pane else { continue };
                let Some(tab) = active_tab else { continue };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let kind_positions: Vec<usize> = children
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| tab_q.contains(*e))
                    .map(|(i, _)| i)
                    .collect();
                let Some(active_idx) = find_kind_index(tab, children, &kind_positions) else {
                    continue;
                };
                let pair = if tab_cmd == TabCommand::SwapPrev {
                    resolve_prev(active_idx)
                } else {
                    resolve_next(active_idx, kind_positions.len())
                };
                if let Some((a, b)) = pair {
                    swap_siblings(&mut commands, pane, children, &kind_positions, a, b);
                }
            }
        }
    }
}

fn close_space_if_only_closing_tab(
    space: Entity,
    closing_tab: Entity,
    spaces: &Query<(Entity, &LastActivatedAt), With<Space>>,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    tab_q: &Query<Entity, With<Tab>>,
    commands: &mut Commands,
) -> bool {
    if entity_tree_contains_tab_other_than(space, closing_tab, all_children, tab_q) {
        return false;
    }
    let siblings = sibling_spaces(space, spaces, child_of_q, all_children);
    if siblings.len() <= 1 {
        return false;
    }
    if let Some(next) = pick_space_after_close(space, &siblings) {
        commands.entity(next).insert(LastActivatedAt::now());
    }
    commands.entity(space).despawn();
    true
}

fn entity_tree_contains_tab_other_than(
    entity: Entity,
    ignored_tab: Entity,
    all_children: &Query<&Children>,
    tab_q: &Query<Entity, With<Tab>>,
) -> bool {
    (tab_q.contains(entity) && entity != ignored_tab)
        || all_children.get(entity).is_ok_and(|children| {
            children.iter().any(|child| {
                entity_tree_contains_tab_other_than(child, ignored_tab, all_children, tab_q)
            })
        })
}

fn sibling_spaces(
    space: Entity,
    spaces: &Query<(Entity, &LastActivatedAt), With<Space>>,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
) -> Vec<Entity> {
    let Ok(parent) = child_of_q.get(space).map(Relationship::get) else {
        return vec![space];
    };
    let Ok(children) = all_children.get(parent) else {
        return vec![space];
    };
    children.iter().filter(|e| spaces.get(*e).is_ok()).collect()
}

fn pick_space_after_close(active: Entity, siblings: &[Entity]) -> Option<Entity> {
    if siblings.len() <= 1 {
        return None;
    }
    let idx = siblings.iter().position(|e| *e == active)?;
    let next_idx = if idx + 1 < siblings.len() { idx + 1 } else { 0 };
    let target = siblings[next_idx];
    if target == active { None } else { Some(target) }
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
                    let target = if Some(entity) == active {
                        ZIndex(1)
                    } else {
                        ZIndex(0)
                    };
                    if *z != target {
                        *z = target;
                    }
                }
            }
        }
    }
}

pub(crate) fn open_command_bar_if_no_tabs(
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    tab_q: Query<Entity, With<Tab>>,
    closing_primary: Query<(), (With<PrimaryWindow>, With<ClosingWindow>)>,
    mut new_tab_ctx: ResMut<NewTabContext>,
    mut commands: Commands,
) {
    if !closing_primary.is_empty() {
        return;
    }
    let (active_space, active_pane, _) = focused_tab(
        &spaces,
        &all_children,
        &leaf_panes,
        &pane_ts,
        &pane_children,
        &tab_ts,
    );
    if active_space.is_some_and(|space| entity_tree_contains_tab(space, &all_children, &tab_q)) {
        return;
    }
    let Some(pane) = active_pane.or_else(|| leaf_panes.iter().next()) else {
        return;
    };
    let tab = commands
        .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    new_tab_ctx.tab = Some(tab);
    new_tab_ctx.previous_tab = None;
    new_tab_ctx.needs_open = true;
}

fn entity_tree_contains_tab(
    entity: Entity,
    all_children: &Query<&Children>,
    tab_q: &Query<Entity, With<Tab>>,
) -> bool {
    tab_q.contains(entity)
        || all_children.get(entity).is_ok_and(|children| {
            children
                .iter()
                .any(|child| entity_tree_contains_tab(child, all_children, tab_q))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        command::{CommandPlugin, WriteAppCommands},
        settings::{
            AppSettings, BrowserSettings, FocusRingSettings, LayoutSettings, PaneSettings,
            ShortcutSettings, SideSheetSettings, WindowSettings,
        },
    };
    use bevy::window::ClosingWindow;

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings {
                    gap: 0.0,
                    radius: 0.0,
                },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
        }
    }

    #[test]
    fn closing_last_tab_opens_command_bar_with_replacement_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.init_resource::<NewTabContext>();
        app.init_resource::<PendingCursorWarp>();
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.add_systems(Update, handle_tab_commands.in_set(WriteAppCommands));

        let window = app.world_mut().spawn(PrimaryWindow).id();
        let space = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(space)))
            .id();
        let original_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Tab(TabCommand::Close));

        app.update();

        assert!(!app.world().entity(window).contains::<ClosingWindow>());
        assert!(app.world().get_entity(original_tab).is_err());

        let ctx = app.world().resource::<NewTabContext>();
        let Some(replacement_tab) = ctx.tab else {
            panic!("expected replacement tab to open command bar");
        };
        assert!(ctx.needs_open);
        assert_eq!(ctx.previous_tab, None);
        assert_eq!(
            app.world()
                .get::<ChildOf>(replacement_tab)
                .map(Relationship::get),
            Some(pane)
        );
    }

    #[test]
    fn closing_last_tab_in_space_closes_space_when_another_space_exists() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.init_resource::<NewTabContext>();
        app.init_resource::<PendingCursorWarp>();
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.add_systems(Update, handle_tab_commands.in_set(WriteAppCommands));

        let root = app.world_mut().spawn_empty().id();
        let remaining_space = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt(1), ChildOf(root)))
            .id();
        let remaining_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(remaining_space)))
            .id();
        app.world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(remaining_pane)));

        let closing_space = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt(2), ChildOf(root)))
            .id();
        let closing_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(closing_space)))
            .id();
        app.world_mut()
            .spawn((Tab::default(), LastActivatedAt(2), ChildOf(closing_pane)));

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Tab(TabCommand::Close));

        app.update();

        assert!(app.world().get_entity(closing_space).is_err());
        assert!(app.world().get_entity(remaining_space).is_ok());
        assert!(
            app.world()
                .get::<LastActivatedAt>(remaining_space)
                .unwrap()
                .0
                > 1
        );

        let ctx = app.world().resource::<NewTabContext>();
        assert_eq!(ctx.tab, None);
        assert!(!ctx.needs_open);
    }

    #[test]
    fn empty_active_pane_opens_command_bar_even_when_other_spaces_have_tabs() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NewTabContext>();
        app.add_systems(Update, open_command_bar_if_no_tabs);

        let old_space = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt(1)))
            .id();
        let old_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(old_space)))
            .id();
        app.world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(old_pane)));

        let active_space = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt(2)))
            .id();
        let active_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(active_space)))
            .id();

        app.update();

        let ctx = app.world().resource::<NewTabContext>();
        let Some(new_tab) = ctx.tab else {
            panic!("expected empty active pane to get pending tab");
        };
        assert!(ctx.needs_open);
        assert_eq!(
            app.world().get::<ChildOf>(new_tab).map(Relationship::get),
            Some(active_pane)
        );
    }

    #[test]
    fn empty_active_pane_does_not_open_command_bar_when_space_has_tabs() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NewTabContext>();
        app.add_systems(Update, open_command_bar_if_no_tabs);

        let space = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt(1)))
            .id();
        let pane_with_tab = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
            .id();
        app.world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(pane_with_tab)));
        app.world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(space)));

        app.update();

        let ctx = app.world().resource::<NewTabContext>();
        assert_eq!(ctx.tab, None);
        assert!(!ctx.needs_open);
    }

    #[test]
    fn active_empty_tab_does_not_reopen_command_bar() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NewTabContext>();
        app.add_systems(Update, open_command_bar_if_no_tabs);

        let space = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(pane)))
            .id();

        app.update();

        let ctx = app.world().resource::<NewTabContext>();
        assert_ne!(ctx.tab, Some(tab));
        assert!(!ctx.needs_open);
    }
}
