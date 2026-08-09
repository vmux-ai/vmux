use crate::event::SERVICES_PAGE_URL;
use crate::{
    NewStackContext,
    pane::{Pane, PaneSplit, PendingCursorWarp, first_leaf_descendant, first_stack_in_pane},
    swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
    tab::{CloseTabRequest, Tab},
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

pub struct StackPlugin;

impl Plugin for StackPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Stack>()
            .init_resource::<FocusedStack>()
            .add_message::<CloseStackRequest>()
            .add_systems(
                Update,
                (
                    handle_stack_commands
                        .in_set(ReadAppCommands)
                        .in_set(StackCommandSet),
                    handle_close_stack_requests.in_set(ReadAppCommands),
                ),
            )
            .add_systems(
                Update,
                compute_focused_stack
                    .in_set(ComputeFocusSet)
                    .after(ReadAppCommands)
                    .after(crate::active::ensure_active_tab)
                    .after(crate::active::ensure_active_stack)
                    .after(crate::active::ensure_active_branch),
            )
            .add_systems(PostUpdate, sync_stack_picking);
    }
}

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

/// Close (despawn) a specific stack entity. Used by agent auto-tidy. Ignored if
/// it is the only stack in its pane, so tidy can never empty (and collapse) a pane.
#[derive(Message, Clone, Copy)]
pub struct CloseStackRequest {
    pub stack: Entity,
}

/// System set for `handle_stack_commands`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StackCommandSet;

fn handle_close_stack_requests(
    mut reader: MessageReader<CloseStackRequest>,
    child_of_q: Query<&ChildOf>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut commands: Commands,
) {
    for req in reader.read() {
        let Ok(pane) = child_of_q.get(req.stack).map(Relationship::get) else {
            continue;
        };
        let Ok(children) = pane_children.get(pane) else {
            continue;
        };
        let stack_count = children.iter().filter(|&e| stack_q.contains(e)).count();
        if stack_count <= 1 {
            continue;
        }
        if new_stack_ctx.stack == Some(req.stack) {
            new_stack_ctx.stack = None;
        }
        if new_stack_ctx.previous_stack == Some(req.stack) {
            new_stack_ctx.previous_stack = None;
        }
        commands.entity(req.stack).despawn();
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
    // Only write when the focus actually changed. An unconditional `ResMut` write
    // marks `FocusedStack` changed every frame, which made `sync_live_start_pages`
    // re-emit the `vmux://start` payload every frame — re-rendering the launcher
    // input and eating keystrokes.
    if cached.tab != tab || cached.pane != pane || cached.stack != stack {
        cached.tab = tab;
        cached.pane = pane;
        cached.stack = stack;
    }
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
    mut close_tab_requests: MessageWriter<CloseTabRequest>,
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
                            &all_children,
                            &stack_q,
                            &mut close_tab_requests,
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
    all_children: &Query<&Children>,
    stack_q: &Query<Entity, With<Stack>>,
    close_tab_requests: &mut MessageWriter<CloseTabRequest>,
) -> bool {
    if entity_tree_contains_stack_other_than(tab, closing_stack, all_children, stack_q) {
        return false;
    }
    close_tab_requests.write(CloseTabRequest { tab });
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
#[path = "stack.test.rs"]
mod tests;
