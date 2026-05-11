use crate::event::{PROCESSES_WEBVIEW_URL, TERMINAL_WEBVIEW_URL};
use crate::{
    LayoutSpawnRequest, NewStackContext,
    pane::{Pane, PaneSplit, PendingCursorWarp, first_leaf_descendant, first_stack_in_pane},
    swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
    tab::Tab,
};
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    window::{ClosingWindow, PrimaryWindow},
};
use moonshine_save::prelude::*;
use vmux_command::{
    AppCommand, LayoutCommand, ReadAppCommands, ServiceCommand, StackCommand, TerminalCommand,
};
use vmux_history::LastActivatedAt;

/// Cached result of `focused_stack()`, computed once per frame in `Update`
/// after all command handlers. Read by push/sync systems to avoid redundant
/// tree walks.
#[derive(Resource, Default)]
pub struct FocusedStack {
    pub tab: Option<Entity>,
    pub pane: Option<Entity>,
    pub stack: Option<Entity>,
}

/// System set for `compute_focused_stack`. Systems that read `Res<FocusedStack>`
/// should be ordered `.after(ComputeFocusSet)` in `Update`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComputeFocusSet;

/// Marker: tab is waiting for close confirmation dialog.
#[derive(Component)]
pub struct PendingStackClose;

/// Marker: close was confirmed, skip dialog next time.
#[derive(Component)]
pub struct CloseConfirmed;

/// System set for `handle_stack_commands`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StackCommandSet;

pub struct StackPlugin;

impl Plugin for StackPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Stack>()
            .init_resource::<FocusedStack>()
            .add_systems(
                Update,
                handle_stack_commands
                    .in_set(ReadAppCommands)
                    .in_set(StackCommandSet),
            )
            .add_systems(
                Update,
                compute_focused_stack
                    .in_set(ComputeFocusSet)
                    .after(ReadAppCommands),
            )
            .add_systems(PostUpdate, sync_stack_picking);
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::stack"]
#[require(Save)]
pub struct Stack {
    pub scroll_x: f32,
    pub scroll_y: f32,
}

/// Returns the entity with the highest `LastActivatedAt` timestamp.
pub fn active_among<'a>(
    entities: impl Iterator<Item = (Entity, &'a LastActivatedAt)>,
) -> Option<Entity> {
    entities.max_by_key(|(_, ts)| ts.0).map(|(e, _)| e)
}

/// Recursively collects leaf panes (panes without PaneSplit) under `root`.
pub fn collect_leaf_panes(
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

pub fn active_pane_in_tab(
    tab: Entity,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: &Query<(Entity, &LastActivatedAt), With<Pane>>,
) -> Option<Entity> {
    let mut panes = Vec::new();
    collect_leaf_panes(tab, all_children, leaf_panes, &mut panes);
    active_among(panes.iter().filter_map(|&e| pane_ts.get(e).ok()))
}

/// Find the active tab (max LastActivatedAt) in a pane.
pub fn active_stack_in_pane(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    tab_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
) -> Option<Entity> {
    pane_children
        .get(pane)
        .ok()
        .and_then(|children| active_among(children.iter().filter_map(|e| tab_ts.get(e).ok())))
}

pub fn focused_stack(
    tabs: &Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: &Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
) -> (Option<Entity>, Option<Entity>, Option<Entity>) {
    let tab = active_among(tabs.iter());
    let pane = tab.and_then(|t| active_pane_in_tab(t, all_children, leaf_panes, pane_ts));
    let stack = pane.and_then(|p| active_stack_in_pane(p, pane_children, stack_ts));
    (tab, pane, stack)
}

fn compute_focused_stack(
    mut cached: ResMut<FocusedStack>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
) {
    let (tab, pane, stack) = focused_stack(
        &tabs,
        &all_children,
        &leaf_panes,
        &pane_ts,
        &pane_children,
        &stack_ts,
    );
    cached.tab = tab;
    cached.pane = pane;
    cached.stack = stack;
}

pub fn stack_bundle() -> impl Bundle {
    (
        Stack::default(),
        vmux_core::PageMetadata::default(),
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

fn handle_stack_commands(
    mut reader: MessageReader<AppCommand>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: Query<Entity, With<Stack>>,
    child_of_q: Query<&ChildOf>,
    split_dir_q: Query<&PaneSplit>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,

    mut new_stack_ctx: ResMut<NewStackContext>,
    mut spawn_requests: MessageWriter<LayoutSpawnRequest>,
    mut commands: Commands,
    mut pending_cursor_warp: ResMut<PendingCursorWarp>,
) {
    for cmd in reader.read() {
        let (stack_cmd, is_terminal, is_processes) = match *cmd {
            AppCommand::Layout(LayoutCommand::Stack(t)) => (t, false, false),
            AppCommand::Terminal(TerminalCommand::New) => (StackCommand::New, true, false),
            AppCommand::Service(ServiceCommand::Open) => (StackCommand::New, false, true),
            _ => continue,
        };

        let (active_tab, active_pane, active_stack) = focused_stack(
            &tabs,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );

        match stack_cmd {
            StackCommand::New => {
                let Some(pane) = active_pane else {
                    continue;
                };
                if is_terminal {
                    let stack = commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id();
                    commands.entity(stack).insert(vmux_core::PageMetadata {
                        url: TERMINAL_WEBVIEW_URL.to_string(),
                        title: "Terminal".to_string(),
                        ..default()
                    });
                    spawn_requests.write(LayoutSpawnRequest::Terminal { stack });
                } else if is_processes {
                    let stack = commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id();
                    commands.entity(stack).insert(vmux_core::PageMetadata {
                        url: PROCESSES_WEBVIEW_URL.to_string(),
                        title: "Background Services".to_string(),
                        ..default()
                    });
                    spawn_requests.write(LayoutSpawnRequest::ProcessesMonitor { stack });
                } else {
                    if new_stack_ctx.stack.is_some() {
                        new_stack_ctx.needs_open = true;
                        continue;
                    }
                    let stack = commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id();
                    new_stack_ctx.stack = Some(stack);
                    new_stack_ctx.previous_stack = active_stack;
                    let url = effective_startup_url
                        .as_deref()
                        .map(|u| u.0.clone())
                        .unwrap_or_default();
                    if url.is_empty() {
                        new_stack_ctx.needs_open = true;
                    } else {
                        spawn_requests.write(LayoutSpawnRequest::OpenUrl { stack, url });
                    }
                }
            }
            StackCommand::Close => {
                let Some(pane) = active_pane else {
                    continue;
                };
                let Some(active) = active_stack else {
                    continue;
                };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let stacks_in_pane: Vec<Entity> =
                    children.iter().filter(|&e| stack_q.contains(e)).collect();
                if stacks_in_pane.len() <= 1 {
                    if let Some(tab) = active_tab
                        && close_tab_if_only_closing_stack(
                            tab,
                            active,
                            &tabs,
                            &child_of_q,
                            &all_children,
                            &stack_q,
                            &mut commands,
                        )
                    {
                        if new_stack_ctx.stack == Some(active) {
                            new_stack_ctx.stack = None;
                        }
                        new_stack_ctx.previous_stack = None;
                        new_stack_ctx.needs_open = false;
                        continue;
                    }

                    if leaf_panes.iter().count() <= 1 {
                        commands.entity(active).despawn();
                        let stack = commands
                            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                            .id();
                        new_stack_ctx.stack = Some(stack);
                        new_stack_ctx.previous_stack = None;
                        new_stack_ctx.needs_open = true;
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
                    let new_stack =
                        active_stack_in_pane(new_active_pane, &pane_children, &stack_ts)
                            .or_else(|| {
                                first_stack_in_pane(new_active_pane, &pane_children, &stack_q)
                            })
                            .or_else(|| {
                                sibling_children
                                    .iter()
                                    .copied()
                                    .find(|&e| stack_q.contains(e))
                            });
                    if let Some(t) = new_stack {
                        commands.entity(t).insert(LastActivatedAt::now());
                    }
                    continue;
                }
                let next = active_among(
                    stacks_in_pane
                        .iter()
                        .filter(|&&e| e != active)
                        .filter_map(|&e| stack_ts.get(e).ok()),
                )
                .unwrap();
                commands.entity(active).despawn();
                commands.entity(next).insert(LastActivatedAt::now());
            }
            StackCommand::Next | StackCommand::Previous => {
                let empty_stack = new_stack_ctx.stack.take();
                let prev_stack = new_stack_ctx.previous_stack.take();
                if let Some(e) = empty_stack {
                    commands.entity(e).despawn();
                }

                let Some(active_tab_e) = active_among(tabs.iter()) else {
                    continue;
                };
                let mut tab_panes = Vec::new();
                collect_leaf_panes(active_tab_e, &all_children, &leaf_panes, &mut tab_panes);
                let mut flat: Vec<(Entity, Entity)> = Vec::new();
                for &pane_e in &tab_panes {
                    if let Ok(children) = pane_children.get(pane_e) {
                        for child in children.iter() {
                            if stack_q.contains(child) && Some(child) != empty_stack {
                                flat.push((pane_e, child));
                            }
                        }
                    }
                }
                if flat.len() < 2 {
                    continue;
                }
                let effective_current = if empty_stack.is_some() {
                    prev_stack.or(active_stack)
                } else {
                    active_stack
                };
                let Some(current) = flat.iter().position(|&(_, t)| Some(t) == effective_current)
                else {
                    continue;
                };
                let delta: i32 = if stack_cmd == StackCommand::Next {
                    1
                } else {
                    -1
                };
                let n = flat.len() as i32;
                let idx = (current as i32 + delta).rem_euclid(n) as usize;
                let (target_pane, target_stack) = flat[idx];
                commands.entity(target_stack).insert(LastActivatedAt::now());
                if active_pane != Some(target_pane) {
                    commands.entity(target_pane).insert(LastActivatedAt::now());
                    pending_cursor_warp.target = Some(target_pane);
                }
            }
            StackCommand::Reopen | StackCommand::Duplicate | StackCommand::MoveToPane => {}
            StackCommand::SwapPrev | StackCommand::SwapNext => {
                let Some(pane) = active_pane else { continue };
                let Some(stack) = active_stack else { continue };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let kind_positions: Vec<usize> = children
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| stack_q.contains(*e))
                    .map(|(i, _)| i)
                    .collect();
                let Some(active_idx) = find_kind_index(stack, children, &kind_positions) else {
                    continue;
                };
                let pair = if stack_cmd == StackCommand::SwapPrev {
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

fn close_tab_if_only_closing_stack(
    tab: Entity,
    closing_stack: Entity,
    tabs: &Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    stack_q: &Query<Entity, With<Stack>>,
    commands: &mut Commands,
) -> bool {
    if entity_tree_contains_stack_other_than(tab, closing_stack, all_children, stack_q) {
        return false;
    }
    let siblings = sibling_tabs(tab, tabs, child_of_q, all_children);
    if siblings.len() <= 1 {
        return false;
    }
    if let Some(next) = pick_tab_after_close(tab, &siblings) {
        commands.entity(next).insert(LastActivatedAt::now());
    }
    commands.entity(tab).despawn();
    true
}

fn entity_tree_contains_stack_other_than(
    entity: Entity,
    ignored_stack: Entity,
    all_children: &Query<&Children>,
    stack_q: &Query<Entity, With<Stack>>,
) -> bool {
    (stack_q.contains(entity) && entity != ignored_stack)
        || all_children.get(entity).is_ok_and(|children| {
            children.iter().any(|child| {
                entity_tree_contains_stack_other_than(child, ignored_stack, all_children, stack_q)
            })
        })
}

fn sibling_tabs(
    tab: Entity,
    tabs: &Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
) -> Vec<Entity> {
    let Ok(parent) = child_of_q.get(tab).map(Relationship::get) else {
        return vec![tab];
    };
    let Ok(children) = all_children.get(parent) else {
        return vec![tab];
    };
    children.iter().filter(|e| tabs.get(*e).is_ok()).collect()
}

fn pick_tab_after_close(active: Entity, siblings: &[Entity]) -> Option<Entity> {
    if siblings.len() <= 1 {
        return None;
    }
    let idx = siblings.iter().position(|e| *e == active)?;
    let next_idx = if idx + 1 < siblings.len() { idx + 1 } else { 0 };
    let target = siblings[next_idx];
    if target == active { None } else { Some(target) }
}

fn sync_stack_picking(
    pane_children: Query<&Children, With<Pane>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut stacks: Query<(Entity, &mut ZIndex), With<Stack>>,
) {
    for pane in &leaf_panes {
        let active = active_stack_in_pane(pane, &pane_children, &stack_ts);
        if let Ok(children) = pane_children.get(pane) {
            for child in children.iter() {
                if let Ok((entity, mut z)) = stacks.get_mut(child) {
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

pub fn open_command_bar_if_no_stacks(
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: Query<Entity, With<Stack>>,
    closing_primary: Query<(), (With<PrimaryWindow>, With<ClosingWindow>)>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut commands: Commands,
) {
    if !closing_primary.is_empty() {
        return;
    }
    let (active_tab, active_pane, _) = focused_stack(
        &tabs,
        &all_children,
        &leaf_panes,
        &pane_ts,
        &pane_children,
        &stack_ts,
    );
    if active_tab.is_some_and(|tab| entity_tree_contains_stack(tab, &all_children, &stack_q)) {
        return;
    }
    let Some(pane) = active_pane.or_else(|| leaf_panes.iter().next()) else {
        return;
    };
    let stack = commands
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    new_stack_ctx.stack = Some(stack);
    new_stack_ctx.previous_stack = None;
    new_stack_ctx.needs_open = true;
}

fn entity_tree_contains_stack(
    entity: Entity,
    all_children: &Query<&Children>,
    stack_q: &Query<Entity, With<Stack>>,
) -> bool {
    stack_q.contains(entity)
        || all_children.get(entity).is_ok_and(|children| {
            children
                .iter()
                .any(|child| entity_tree_contains_stack(child, all_children, stack_q))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use bevy::window::ClosingWindow;
    use bevy_cef::prelude::WebviewExtendStandardMaterial;
    use vmux_command::{CommandPlugin, WriteAppCommands};

    fn test_settings() -> LayoutSettings {
        LayoutSettings {
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
        }
    }

    #[test]
    fn closing_last_tab_opens_command_bar_with_replacement_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_message::<LayoutSpawnRequest>();
        app.init_resource::<NewStackContext>();
        app.init_resource::<PendingCursorWarp>();
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.add_systems(Update, handle_stack_commands.in_set(WriteAppCommands));

        let window = app.world_mut().spawn(PrimaryWindow).id();
        let tab_e = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab_e)))
            .id();
        let original_tab = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));

        app.update();

        assert!(!app.world().entity(window).contains::<ClosingWindow>());
        assert!(app.world().get_entity(original_tab).is_err());

        let ctx = app.world().resource::<NewStackContext>();
        let Some(replacement_tab) = ctx.stack else {
            panic!("expected replacement tab to open command bar");
        };
        assert!(ctx.needs_open);
        assert_eq!(ctx.previous_stack, None);
        assert_eq!(
            app.world()
                .get::<ChildOf>(replacement_tab)
                .map(Relationship::get),
            Some(pane)
        );
    }

    #[test]
    fn closing_last_stack_in_tab_closes_tab_when_another_tab_exists() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_message::<LayoutSpawnRequest>();
        app.init_resource::<NewStackContext>();
        app.init_resource::<PendingCursorWarp>();
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.add_systems(Update, handle_stack_commands.in_set(WriteAppCommands));

        let root = app.world_mut().spawn_empty().id();
        let remaining_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(root)))
            .id();
        let remaining_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(remaining_tab)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            LastActivatedAt(1),
            ChildOf(remaining_pane),
        ));

        let closing_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(2), ChildOf(root)))
            .id();
        let closing_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(closing_tab)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(closing_pane)));

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));

        app.update();

        assert!(app.world().get_entity(closing_tab).is_err());
        assert!(app.world().get_entity(remaining_tab).is_ok());
        assert!(app.world().get::<LastActivatedAt>(remaining_tab).unwrap().0 > 1);

        let ctx = app.world().resource::<NewStackContext>();
        assert_eq!(ctx.stack, None);
        assert!(!ctx.needs_open);
    }

    #[test]
    fn empty_active_pane_opens_command_bar_even_when_other_tabs_have_stacks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NewStackContext>();
        app.add_systems(Update, open_command_bar_if_no_stacks);

        let old_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let old_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(old_tab)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(old_pane)));

        let active_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(2)))
            .id();
        let active_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(active_tab)))
            .id();

        app.update();

        let ctx = app.world().resource::<NewStackContext>();
        let Some(new_stack) = ctx.stack else {
            panic!("expected empty active pane to get pending stack");
        };
        assert!(ctx.needs_open);
        assert_eq!(
            app.world().get::<ChildOf>(new_stack).map(Relationship::get),
            Some(active_pane)
        );
    }

    #[test]
    fn empty_active_pane_does_not_open_command_bar_when_tab_has_stacks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NewStackContext>();
        app.add_systems(Update, open_command_bar_if_no_stacks);

        let tab_e = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let pane_with_stack = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(tab_e)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            LastActivatedAt(1),
            ChildOf(pane_with_stack),
        ));
        app.world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(tab_e)));

        app.update();

        let ctx = app.world().resource::<NewStackContext>();
        assert_eq!(ctx.stack, None);
        assert!(!ctx.needs_open);
    }

    #[test]
    fn active_empty_stack_does_not_reopen_command_bar() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NewStackContext>();
        app.add_systems(Update, open_command_bar_if_no_stacks);

        let tab_e = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(tab_e)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
            .id();

        app.update();

        let ctx = app.world().resource::<NewStackContext>();
        assert_ne!(ctx.stack, Some(stack));
        assert!(!ctx.needs_open);
    }
}
