use crate::event::SERVICES_PAGE_URL;
use crate::{
    NewStackContext,
    pane::{Pane, PaneSplit, PendingCursorWarp, first_leaf_descendant, first_stack_in_pane},
    swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
    tab::Tab,
};
use bevy::{
    ecs::{relationship::Relationship, system::SystemParam},
    prelude::*,
    window::{ClosingWindow, PrimaryWindow},
};
use moonshine_save::prelude::*;
use vmux_command::{
    AppCommand, BrowserCommand, LayoutCommand, OpenCommand, ReadAppCommands, ServiceCommand,
    StackCommand,
};
use vmux_core::{PageOpenRequest, PageOpenTarget};
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

#[derive(SystemParam)]
pub struct ActiveTabParam<'w, 's> {
    tabs: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Tab>>,
    active_tabs: Query<'w, 's, Entity, (With<Tab>, With<vmux_core::Active>)>,
    active_spaces: Query<'w, 's, (), (With<crate::space::Space>, With<vmux_core::Active>)>,
    child_of: Query<'w, 's, &'static ChildOf>,
}

impl ActiveTabParam<'_, '_> {
    pub fn get(&self) -> Option<Entity> {
        let scoped = self.active_tabs.iter().find(|&tab| {
            self.child_of
                .get(tab)
                .ok()
                .map(|co| self.active_spaces.get(co.parent()).is_ok())
                .unwrap_or(false)
        });
        if scoped.is_some() {
            return scoped;
        }
        // No active tab is scoped to an active space — e.g. on a fresh start
        // before the default tab is adopted into / marked active within its
        // space. Fall back to the global most-recently-active tab so callers
        // (notably `open_startup_url_if_no_stacks`) don't treat the layout as
        // empty and respawn startup content every frame.
        active_among(self.tabs.iter())
    }
}

pub fn focused_stack(
    active_tab: Option<Entity>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: &Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
) -> (Option<Entity>, Option<Entity>, Option<Entity>) {
    let pane = active_tab.and_then(|t| active_pane_in_tab(t, all_children, leaf_panes, pane_ts));
    let stack = pane.and_then(|p| active_stack_in_pane(p, pane_children, stack_ts));
    (active_tab, pane, stack)
}

fn compute_focused_stack(
    mut cached: ResMut<FocusedStack>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
) {
    let tab = active_tab_param.get();
    let pane = tab.and_then(|t| active_pane_in_tab(t, &all_children, &leaf_panes, &pane_ts));
    let stack = pane.and_then(|p| active_stack_in_pane(p, &pane_children, &stack_ts));
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
    active_tab_param: ActiveTabParam,
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
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
    mut pending_cursor_warp: ResMut<PendingCursorWarp>,
) {
    for cmd in reader.read() {
        enum Dispatch {
            Stack(StackCommand),
            NewStackServices,
            NewStackUrl(Option<String>),
        }

        let dispatch = match cmd {
            AppCommand::Layout(LayoutCommand::Stack(t)) => Dispatch::Stack(*t),
            AppCommand::Service(ServiceCommand::Open) => Dispatch::NewStackServices,
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url })) => {
                Dispatch::NewStackUrl(url.clone())
            }
            _ => continue,
        };

        let (active_tab, active_pane, active_stack) = focused_stack(
            active_tab_param.get(),
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );

        match dispatch {
            Dispatch::NewStackServices => {
                let Some(pane) = active_pane else {
                    continue;
                };
                let stack = commands
                    .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                    .id();
                commands.entity(stack).insert(vmux_core::PageMetadata {
                    url: SERVICES_PAGE_URL.to_string(),
                    title: "Background Services".to_string(),
                    bg_color: Some(crate::event::TERMINAL_CEF_BG_COLOR.to_string()),
                    ..default()
                });
                page_open_requests.write(PageOpenRequest {
                    target: PageOpenTarget::Stack(stack),
                    url: SERVICES_PAGE_URL.to_string(),
                    request_id: None,
                });
            }
            Dispatch::NewStackUrl(override_url) => {
                let Some(pane) = active_pane else {
                    continue;
                };
                let startup = effective_startup_url
                    .as_deref()
                    .map(|u| u.0.clone())
                    .filter(|u| !u.is_empty());
                let resolved = override_url.filter(|u| !u.is_empty()).or(startup);
                if let Some(url) = resolved {
                    let stack = commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id();
                    page_open_requests.write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack),
                        url,
                        request_id: None,
                    });
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
                    new_stack_ctx.needs_open = true;
                }
            }
            Dispatch::Stack(StackCommand::Close) => {
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

                    if let Ok(parent) = child_of_q.get(pane).map(Relationship::get)
                        && split_dir_q.contains(parent)
                    {
                        commands.entity(active).despawn();
                        let Ok(siblings) = pane_children.get(parent) else {
                            continue;
                        };
                        let pane_siblings: Vec<Entity> = siblings
                            .iter()
                            .filter(|&e| {
                                e != pane && (leaf_panes.contains(e) || split_dir_q.contains(e))
                            })
                            .collect();

                        if pane_siblings.len() >= 2 {
                            commands.entity(pane).despawn();
                            let new_active_pane = pane_siblings
                                .iter()
                                .copied()
                                .max_by_key(|&e| pane_ts.get(e).map(|(_, t)| t.0).unwrap_or(0))
                                .unwrap_or(pane_siblings[0]);
                            let focus_leaf =
                                first_leaf_descendant(new_active_pane, &pane_children, &leaf_panes);
                            commands.entity(focus_leaf).insert(LastActivatedAt::now());
                            if let Some(t) =
                                active_stack_in_pane(focus_leaf, &pane_children, &stack_ts).or_else(
                                    || first_stack_in_pane(focus_leaf, &pane_children, &stack_q),
                                )
                            {
                                commands.entity(t).insert(LastActivatedAt::now());
                            }
                            if new_stack_ctx.stack == Some(active) {
                                new_stack_ctx.stack = None;
                            }
                            new_stack_ctx.previous_stack = None;
                            new_stack_ctx.needs_open = false;
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
                            new_active_pane =
                                first_leaf_descendant(sibling, &pane_children, &leaf_panes);
                            commands.entity(sibling).remove::<ChildOf>();
                            commands.queue(move |world: &mut World| {
                                world.despawn(sibling);
                                crate::pane::set_pane_split_direction(
                                    world,
                                    parent,
                                    sibling_direction,
                                );
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
                        if new_stack_ctx.stack == Some(active) {
                            new_stack_ctx.stack = None;
                        }
                        new_stack_ctx.previous_stack = None;
                        new_stack_ctx.needs_open = false;
                        continue;
                    }

                    commands.entity(active).despawn();
                    let stack = commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id();
                    new_stack_ctx.previous_stack = None;
                    let startup_url = effective_startup_url
                        .as_deref()
                        .map(|u| u.0.clone())
                        .unwrap_or_default();
                    if startup_url.is_empty() {
                        new_stack_ctx.stack = Some(stack);
                        new_stack_ctx.needs_open = true;
                    } else {
                        new_stack_ctx.stack = None;
                        new_stack_ctx.needs_open = false;
                        page_open_requests.write(PageOpenRequest {
                            target: PageOpenTarget::Stack(stack),
                            url: startup_url,
                            request_id: None,
                        });
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
            Dispatch::Stack(sc @ (StackCommand::Next | StackCommand::Previous)) => {
                let empty_stack = new_stack_ctx.stack.take();
                let prev_stack = new_stack_ctx.previous_stack.take();
                if let Some(e) = empty_stack {
                    commands.entity(e).despawn();
                }

                let Some(active_tab_e) = active_tab else {
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
                let delta: i32 = if sc == StackCommand::Next { 1 } else { -1 };
                let n = flat.len() as i32;
                let idx = (current as i32 + delta).rem_euclid(n) as usize;
                let (target_pane, target_stack) = flat[idx];
                commands.entity(target_stack).insert(LastActivatedAt::now());
                if active_pane != Some(target_pane) {
                    commands.entity(target_pane).insert(LastActivatedAt::now());
                    pending_cursor_warp.target = Some(target_pane);
                }
            }
            Dispatch::Stack(
                StackCommand::Reopen | StackCommand::Duplicate | StackCommand::MoveToPane,
            ) => {}
            Dispatch::Stack(sc @ (StackCommand::SwapPrev | StackCommand::SwapNext)) => {
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
                let pair = if sc == StackCommand::SwapPrev {
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

pub fn open_startup_url_if_no_stacks(
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: Query<Entity, With<Stack>>,
    closing_primary: Query<(), (With<PrimaryWindow>, With<ClosingWindow>)>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    if !closing_primary.is_empty() {
        return;
    }
    let (active_tab, active_pane, _) = focused_stack(
        active_tab_param.get(),
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
    let startup_url = effective_startup_url
        .as_deref()
        .map(|u| u.0.clone())
        .unwrap_or_default();
    if startup_url.is_empty() {
        new_stack_ctx.stack = Some(stack);
        new_stack_ctx.previous_stack = None;
        new_stack_ctx.needs_open = true;
    } else {
        page_open_requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url: startup_url,
            request_id: None,
        });
    }
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
    use bevy::ecs::relationship::Relationship;
    use bevy::window::ClosingWindow;
    use bevy_cef::prelude::WebviewExtendStandardMaterial;
    use vmux_command::{CommandPlugin, WriteAppCommands};

    fn test_settings() -> LayoutSettings {
        LayoutSettings {
            radius: 0.0,
            window: WindowSettings {
                padding: 0.0,
            },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        }
    }

    #[test]
    fn closing_last_stack_without_startup_url_opens_command_bar_with_replacement_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<PendingCursorWarp>()
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_stack_commands.in_set(WriteAppCommands));

        let window = app.world_mut().spawn(PrimaryWindow).id();
        let tab_e = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab_e)))
            .id();
        let original_stack = app
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
        assert!(app.world().get_entity(original_stack).is_err());

        let ctx = app.world().resource::<NewStackContext>();
        let Some(replacement_stack) = ctx.stack else {
            panic!("expected replacement stack to open command bar");
        };
        assert!(ctx.needs_open);
        assert_eq!(ctx.previous_stack, None);
        assert_eq!(
            app.world()
                .get::<ChildOf>(replacement_stack)
                .map(Relationship::get),
            Some(pane)
        );
    }

    #[test]
    fn closing_last_stack_in_tab_closes_the_tab_when_another_tab_exists() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<PendingCursorWarp>()
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_stack_commands.in_set(WriteAppCommands));

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
        let closing_stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(closing_pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));

        app.update();

        assert!(app.world().get_entity(closing_tab).is_err());
        assert!(app.world().get_entity(closing_stack).is_err());
        assert!(app.world().get_entity(remaining_tab).is_ok());
        assert!(app.world().get::<LastActivatedAt>(remaining_tab).unwrap().0 > 1);

        let ctx = app.world().resource::<NewStackContext>();
        assert_eq!(ctx.stack, None);
        assert!(!ctx.needs_open);
    }

    #[test]
    fn closing_only_stack_in_split_pane_closes_pane() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<PendingCursorWarp>()
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_stack_commands.in_set(WriteAppCommands));

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                crate::pane::split_root_bundle(crate::pane::PaneSplitDirection::Row),
                ChildOf(tab),
            ))
            .id();
        let active_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(split)))
            .id();
        let other_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(split)))
            .id();
        let original_stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(active_pane)))
            .id();
        let other_stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(other_pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));

        app.update();

        assert!(app.world().get_entity(split).is_ok());
        assert!(app.world().get_entity(active_pane).is_err());
        assert!(app.world().get_entity(other_pane).is_err());
        assert!(app.world().get_entity(original_stack).is_err());
        assert!(app.world().get_entity(other_stack).is_ok());
        assert_eq!(
            app.world()
                .get::<ChildOf>(other_stack)
                .map(Relationship::get),
            Some(split)
        );
        assert!(!app.world().entity(split).contains::<PaneSplit>());
        assert_eq!(app.world().resource::<NewStackContext>().stack, None);
        assert!(!app.world().resource::<NewStackContext>().needs_open);
    }

    #[test]
    fn closing_stack_in_three_way_split_keeps_split_and_does_not_respawn_startup() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<PendingCursorWarp>()
            .insert_resource(test_settings())
            .insert_resource(crate::settings::EffectiveStartupUrl(
                "vmux://agent/vibe/".to_string(),
            ))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_stack_commands.in_set(WriteAppCommands));

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                crate::pane::split_root_bundle(crate::pane::PaneSplitDirection::Row),
                ChildOf(tab),
            ))
            .id();
        let active_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(3), ChildOf(split)))
            .id();
        let p2 = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(split)))
            .id();
        let p3 = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(split)))
            .id();
        let active_stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(3), ChildOf(active_pane)))
            .id();
        let s2 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(p2)))
            .id();
        let s3 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(p3)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));
        app.update();

        assert!(
            app.world().get_entity(active_pane).is_err(),
            "closed terminal pane is despawned"
        );
        assert!(
            app.world().get_entity(active_stack).is_err(),
            "closed terminal stack is despawned"
        );
        assert!(
            app.world().entity(split).contains::<PaneSplit>(),
            "a 3-way split must stay a split after one terminal closes (tree not corrupted)"
        );
        let children: Vec<Entity> = app
            .world()
            .get::<Children>(split)
            .expect("split has children")
            .iter()
            .collect();
        assert_eq!(children, vec![p2, p3], "exactly the two survivors remain");
        assert!(app.world().get_entity(s2).is_ok() && app.world().get_entity(s3).is_ok());
        let mut stacks = app.world_mut().query_filtered::<Entity, With<Stack>>();
        assert_eq!(
            stacks.iter(app.world()).count(),
            2,
            "no replacement startup (Vibe) stack spawned"
        );
        let reqs: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect();
        assert!(
            reqs.is_empty(),
            "closing a terminal in an N-ary split must not open the startup URL"
        );
    }

    #[test]
    fn empty_active_pane_opens_command_bar_even_when_other_tabs_have_stacks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<NewStackContext>()
            .add_message::<PageOpenRequest>()
            .add_systems(Update, open_startup_url_if_no_stacks);

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
        app.add_plugins(MinimalPlugins)
            .init_resource::<NewStackContext>()
            .add_message::<PageOpenRequest>()
            .add_systems(Update, open_startup_url_if_no_stacks);

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
        app.add_plugins(MinimalPlugins)
            .init_resource::<NewStackContext>()
            .add_message::<PageOpenRequest>()
            .add_systems(Update, open_startup_url_if_no_stacks);

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

    #[derive(Resource, Default)]
    struct CollectedSpawns(Vec<PageOpenRequest>);

    fn collect_spawn_requests(
        mut reader: MessageReader<PageOpenRequest>,
        mut collected: ResMut<CollectedSpawns>,
    ) {
        for req in reader.read() {
            collected.0.push(req.clone());
        }
    }

    fn build_app_with_collector() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<PageOpenRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<PendingCursorWarp>()
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .init_resource::<CollectedSpawns>()
            .add_systems(
                Update,
                (
                    handle_stack_commands.in_set(WriteAppCommands),
                    collect_spawn_requests.after(handle_stack_commands),
                ),
            );
        app
    }

    fn build_hierarchy(app: &mut App) -> (Entity, Entity, Entity) {
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack =
            app.world_mut()
                .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));
        (tab, pane, stack.id())
    }

    #[test]
    fn closing_last_stack_uses_startup_url_for_replacement_stack() {
        let mut app = build_app_with_collector();
        app.insert_resource(crate::settings::EffectiveStartupUrl(
            "https://startup.test".into(),
        ));
        let (_tab, pane, original_stack) = build_hierarchy(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));

        app.update();

        assert!(app.world().get_entity(original_stack).is_err());
        let ctx = app.world().resource::<NewStackContext>();
        assert_eq!(ctx.stack, None);
        assert!(!ctx.needs_open);

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        assert_eq!(collected.0[0].url, "https://startup.test");
        match collected.0[0].target {
            PageOpenTarget::Stack(replacement_stack) => {
                assert_ne!(replacement_stack, original_stack);
                assert_eq!(
                    app.world()
                        .get::<ChildOf>(replacement_stack)
                        .map(Relationship::get),
                    Some(pane)
                );
            }
            _ => panic!("expected stack target"),
        }
    }

    #[test]
    fn open_in_new_stack_with_explicit_url() {
        let mut app = build_app_with_collector();
        let (_tab, pane, _stack) = build_hierarchy(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewStack {
                    url: Some("https://example.com".into()),
                },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1, "expected one spawn request");
        match &collected.0[0] {
            PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url,
                ..
            } => {
                assert_eq!(url, "https://example.com");
                assert_eq!(
                    app.world().get::<ChildOf>(*stack).map(Relationship::get),
                    Some(pane),
                );
            }
            other => panic!("expected PageOpenRequest, got {other:?}"),
        }
    }

    #[test]
    fn open_in_new_stack_none_url_queues_empty_stack_for_command_bar() {
        let mut app = build_app_with_collector();
        let (_tab, pane, _stack) = build_hierarchy(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewStack { url: None },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert!(
            collected.0.is_empty(),
            "no spawn request until URL is provided"
        );
        let ctx = app.world().resource::<NewStackContext>();
        let queued = ctx.stack.expect("an empty stack should be queued");
        assert_eq!(
            app.world().get::<ChildOf>(queued).map(Relationship::get),
            Some(pane),
        );
        assert!(ctx.needs_open, "command bar should be requested");
    }

    #[test]
    fn in_new_stack_with_no_url_uses_startup_url() {
        let mut app = build_app_with_collector();
        app.insert_resource(crate::settings::EffectiveStartupUrl(
            "https://startup.test".into(),
        ));
        let (_tab, _pane, _stack) = build_hierarchy(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewStack { url: None },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        assert_eq!(collected.0[0].url, "https://startup.test");
    }

    #[test]
    fn active_tab_param_picks_active_space_tab_not_global_max() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        let main = app.world_mut().spawn(crate::window::Main).id();
        let space_a = app
            .world_mut()
            .spawn((crate::space::Space, ChildOf(main)))
            .id();
        let _tab_a = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt(100),
                ChildOf(space_a),
            ))
            .id();
        let space_b = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)))
            .id();
        let tab_b = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt(1),
                ChildOf(space_b),
            ))
            .id();

        let got = app
            .world_mut()
            .run_system_once(|param: ActiveTabParam| param.get())
            .unwrap();

        assert_eq!(got, Some(tab_b));
    }

    #[test]
    fn active_tab_param_falls_back_to_global_when_no_scoped_active_tab() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        let main = app.world_mut().spawn(crate::window::Main).id();
        // An active space exists, but the only tab isn't scoped to it — the
        // fresh-start state where the default tab is parented under Main before
        // it is adopted into / marked active within its space.
        app.world_mut()
            .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)));
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(5), ChildOf(main)))
            .id();

        let got = app
            .world_mut()
            .run_system_once(|param: ActiveTabParam| param.get())
            .unwrap();

        assert_eq!(
            got,
            Some(tab),
            "must fall back to the global tab so the layout isn't treated as empty (else startup respawns forever)"
        );
    }
}
