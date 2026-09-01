use crate::event::SERVICES_PAGE_URL;
use crate::{
    host::swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
    pane::{Pane, PaneSplit, PendingCursorWarp, first_leaf_descendant, first_stack_in_pane},
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
pub use vmux_core::workspace::{ComputeFocusSet, StackCommandSet};
use vmux_core::{PageOpenRequest, PageOpenTarget};
use vmux_flex::prelude::*;
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
                    handle_close_stack_requests
                        .in_set(ReadAppCommands)
                        .in_set(CloseStackSet),
                )
                    .chain(),
            )
            .add_systems(
                Update,
                compute_focused_stack
                    .in_set(ComputeFocusSet)
                    .after(ReadAppCommands)
                    .after(crate::active::ensure_active_tab)
                    .after(crate::active::ensure_active_stack)
                    .after(crate::active::ensure_active_branch),
            );
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CloseStackSet;

#[derive(Resource, Default)]
pub struct FocusedStack {
    pub tab: Option<Entity>,
    pub pane: Option<Entity>,
    pub stack: Option<Entity>,
}

#[derive(Component)]
pub struct PendingStackClose;

#[derive(Component)]
pub struct CloseConfirmed;

#[derive(Message, Clone, Copy)]
pub struct CloseStackRequest {
    pub stack: Entity,
    pub reason: CloseStackReason,
}

impl CloseStackRequest {
    pub fn by_user(stack: Entity) -> Self {
        Self {
            stack,
            reason: CloseStackReason::ByUser,
        }
    }

    pub fn tidying(stack: Entity) -> Self {
        Self {
            stack,
            reason: CloseStackReason::Tidying,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseStackReason {
    ByUser,
    Tidying,
}

fn handle_close_stack_requests(
    mut reader: MessageReader<CloseStackRequest>,
    mut closer: StackCloser,
    mut commands: Commands,
) {
    for request in reader.read() {
        closer.close(*request, &mut commands);
    }
}

#[derive(SystemParam)]
struct StackCloser<'w, 's> {
    active_tab: ActiveTabParam<'w, 's>,
    all_children: Query<'w, 's, &'static Children>,
    leaf_panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Pane>>,
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    stack_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Stack>>,
    stacks: Query<'w, 's, Entity, With<Stack>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    splits: Query<'w, 's, &'static PaneSplit>,
    startup_url: Option<Res<'w, vmux_core::EffectiveStartupUrl>>,
    close_tab_requests: MessageWriter<'w, CloseTabRequest>,
    page_open_requests: MessageWriter<'w, PageOpenRequest>,
}

impl StackCloser<'_, '_> {
    fn close(&mut self, request: CloseStackRequest, commands: &mut Commands) {
        let Ok(pane) = self.child_of.get(request.stack).map(Relationship::get) else {
            return;
        };
        let Ok(children) = self.pane_children.get(pane) else {
            return;
        };
        let stacks_in_pane: Vec<Entity> = children
            .iter()
            .filter(|&e| self.stacks.contains(e))
            .collect();

        if stacks_in_pane.len() <= 1 {
            if request.reason == CloseStackReason::Tidying {
                return;
            }
            self.close_last_stack_in_pane(pane, request.stack, commands);
            return;
        }

        let was_active =
            active_stack_in_pane(pane, &self.pane_children, &self.stack_ts) == Some(request.stack);
        commands.entity(request.stack).despawn();
        if !was_active {
            return;
        }
        let successor = active_among(
            stacks_in_pane
                .iter()
                .filter(|&&e| e != request.stack)
                .filter_map(|&e| self.stack_ts.get(e).ok()),
        );
        if let Some(successor) = successor {
            commands.entity(successor).insert(LastActivatedAt::now());
        }
    }

    fn close_last_stack_in_pane(&mut self, pane: Entity, stack: Entity, commands: &mut Commands) {
        if let Some(tab) = self.active_tab.get()
            && self.closes_the_tab(tab, stack)
        {
            return;
        }

        let split_parent = match self.child_of.get(pane).map(Relationship::get) {
            Ok(parent) if self.splits.contains(parent) => Some(parent),
            _ => None,
        };
        let Some(parent) = split_parent else {
            commands.entity(stack).despawn();
            let replacement = commands
                .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                .id();
            self.page_open_requests.write(PageOpenRequest {
                target: PageOpenTarget::Stack(replacement),
                url: vmux_core::EffectiveStartupUrl::of(self.startup_url.as_deref()),
                request_id: None,
            });
            return;
        };

        commands.entity(stack).despawn();
        let Ok(siblings) = self.pane_children.get(parent) else {
            return;
        };
        let pane_siblings: Vec<Entity> = siblings
            .iter()
            .filter(|&e| e != pane && (self.leaf_panes.contains(e) || self.splits.contains(e)))
            .collect();

        if pane_siblings.len() >= 2 {
            commands.entity(pane).despawn();
            let new_active_pane = pane_siblings
                .iter()
                .copied()
                .max_by_key(|&e| self.pane_ts.get(e).map(|(_, t)| t.0).unwrap_or(0))
                .unwrap_or(pane_siblings[0]);
            let focus_leaf =
                first_leaf_descendant(new_active_pane, &self.pane_children, &self.leaf_panes);
            commands.entity(focus_leaf).insert(LastActivatedAt::now());
            if let Some(next) = self.first_stack_to_activate(focus_leaf) {
                commands.entity(next).insert(LastActivatedAt::now());
            }
            return;
        }

        let Some(sibling) = pane_siblings.into_iter().next() else {
            return;
        };
        let sibling_children: Vec<Entity> = self
            .pane_children
            .get(sibling)
            .map(|c| c.iter().collect())
            .unwrap_or_default();

        for &child in &sibling_children {
            commands.entity(child).insert(ChildOf(parent));
        }

        let new_active_pane;
        if self.splits.contains(sibling) {
            let sibling_direction = self
                .splits
                .get(sibling)
                .map(|s| s.direction)
                .unwrap_or_default();
            new_active_pane = first_leaf_descendant(sibling, &self.pane_children, &self.leaf_panes);
            commands.entity(sibling).remove::<ChildOf>();
            commands.queue(move |world: &mut World| {
                world.despawn(sibling);
                crate::pane::set_pane_split_direction(world, parent, sibling_direction);
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
        let next = self.first_stack_to_activate(new_active_pane).or_else(|| {
            sibling_children
                .iter()
                .copied()
                .find(|&e| self.stacks.contains(e))
        });
        if let Some(next) = next {
            commands.entity(next).insert(LastActivatedAt::now());
        }
    }

    fn first_stack_to_activate(&self, pane: Entity) -> Option<Entity> {
        active_stack_in_pane(pane, &self.pane_children, &self.stack_ts)
            .or_else(|| first_stack_in_pane(pane, &self.pane_children, &self.stacks))
    }

    fn closes_the_tab(&mut self, tab: Entity, stack: Entity) -> bool {
        if entity_tree_contains_stack_other_than(tab, stack, &self.all_children, &self.stacks) {
            return false;
        }
        self.close_tab_requests.write(CloseTabRequest { tab });
        true
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

pub fn active_among<'a>(
    entities: impl Iterator<Item = (Entity, &'a LastActivatedAt)>,
) -> Option<Entity> {
    entities.max_by_key(|(_, ts)| ts.0).map(|(e, _)| e)
}

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
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
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
    effective_startup_url: Option<Res<vmux_core::EffectiveStartupUrl>>,
    mut close_stack_requests: MessageWriter<CloseStackRequest>,
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
                let url = override_url.filter(|u| !u.is_empty()).unwrap_or_else(|| {
                    vmux_core::EffectiveStartupUrl::of(effective_startup_url.as_deref())
                });
                let stack = commands
                    .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                    .id();
                page_open_requests.write(PageOpenRequest {
                    target: PageOpenTarget::Stack(stack),
                    url,
                    request_id: None,
                });
            }
            Dispatch::Stack(StackCommand::Close) => {
                let Some(active) = active_stack else {
                    continue;
                };
                close_stack_requests.write(CloseStackRequest::by_user(active));
            }
            Dispatch::Stack(sc @ (StackCommand::Next | StackCommand::Previous)) => {
                let Some(active_tab_e) = active_tab else {
                    continue;
                };
                let mut tab_panes = Vec::new();
                collect_leaf_panes(active_tab_e, &all_children, &leaf_panes, &mut tab_panes);
                let mut flat: Vec<(Entity, Entity)> = Vec::new();
                for &pane_e in &tab_panes {
                    if let Ok(children) = pane_children.get(pane_e) {
                        for child in children.iter() {
                            if stack_q.contains(child) {
                                flat.push((pane_e, child));
                            }
                        }
                    }
                }
                if flat.len() < 2 {
                    continue;
                }
                let Some(current) = flat.iter().position(|&(_, t)| Some(t) == active_stack) else {
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

pub fn open_startup_url_if_no_stacks(
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: Query<Entity, With<Stack>>,
    closing_primary: Query<(), (With<PrimaryWindow>, With<ClosingWindow>)>,
    effective_startup_url: Option<Res<vmux_core::EffectiveStartupUrl>>,
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
    page_open_requests.write(PageOpenRequest {
        target: PageOpenTarget::Stack(stack),
        url: vmux_core::EffectiveStartupUrl::of(effective_startup_url.as_deref()),
        request_id: None,
    });
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
    use crate::PendingLaunch;
    use crate::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use bevy::ecs::relationship::Relationship;
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

    fn close_request_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .add_systems(Update, handle_close_stack_requests);
        app
    }

    #[test]
    fn close_stack_request_despawns_target_keeps_siblings() {
        let mut app = close_request_app();

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let s1 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
            .id();
        let s2 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::tidying(s1));
        app.update();

        assert!(app.world().get_entity(s1).is_err(), "target despawned");
        assert!(app.world().get_entity(s2).is_ok(), "sibling kept");
    }

    #[test]
    fn tidying_the_last_stack_in_a_pane_leaves_the_pane_alone() {
        let mut app = close_request_app();

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let only = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::tidying(only));
        app.update();

        assert!(app.world().get_entity(only).is_ok(), "never empties a pane");
        assert!(app.world().get_entity(pane).is_ok(), "the pane survives");
        assert!(app.world().get_entity(tab).is_ok(), "the tab survives");
        assert!(
            app.world_mut()
                .resource_mut::<Messages<CloseTabRequest>>()
                .drain()
                .next()
                .is_none(),
            "an agent tidying its own stack must not close the user's tab"
        );
    }

    #[test]
    fn closing_an_inactive_stack_by_id_leaves_activation_where_it_was() {
        let mut app = close_request_app();

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let first = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
            .id();
        let middle = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(pane)))
            .id();
        let active = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(3), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(middle));
        app.update();

        assert!(
            app.world().get_entity(middle).is_err(),
            "the named stack is the one that dies"
        );
        assert!(app.world().get_entity(first).is_ok());
        assert_eq!(
            app.world().get::<LastActivatedAt>(active).unwrap().0,
            3,
            "closing an inactive stack must not re-stamp the active one"
        );
        assert_eq!(app.world().get::<LastActivatedAt>(first).unwrap().0, 1);
    }

    #[test]
    fn closing_the_active_stack_by_id_activates_the_most_recent_survivor() {
        let mut app = close_request_app();

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let oldest = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
            .id();
        let runner_up = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(pane)))
            .id();
        let active = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(3), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(active));
        app.update();

        assert!(app.world().get_entity(active).is_err());
        let oldest_ts = app.world().get::<LastActivatedAt>(oldest).unwrap().0;
        let runner_up_ts = app.world().get::<LastActivatedAt>(runner_up).unwrap().0;
        assert_eq!(oldest_ts, 1, "the oldest stack must not be activated");
        assert!(
            runner_up_ts > oldest_ts,
            "the most recently used survivor takes over"
        );
    }

    #[test]
    fn focused_stack_not_rewritten_when_focus_is_stable() {
        #[derive(Resource, Default)]
        struct ChangeLog(Vec<bool>);

        fn probe(focused: Res<FocusedStack>, mut log: ResMut<ChangeLog>) {
            log.0.push(focused.is_changed());
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<FocusedStack>()
            .init_resource::<ChangeLog>()
            .add_systems(Update, (compute_focused_stack, probe).chain());

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)))
            .id();

        app.update();
        app.update();
        app.update();

        assert_eq!(
            app.world().resource::<FocusedStack>().stack,
            Some(stack),
            "focus should resolve to the only stack"
        );
        let log = &app.world().resource::<ChangeLog>().0;
        assert_eq!(
            log.last(),
            Some(&false),
            "FocusedStack rewritten on a stable frame; log={log:?}"
        );
    }

    #[test]
    fn closing_last_stack_preloads_fresh_tab_without_workspace_state() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<crate::tab::LastTabCloseAt>()
            .init_resource::<FocusedStack>()
            .insert_resource(test_settings())
            .add_systems(
                Update,
                (
                    handle_stack_commands.in_set(WriteAppCommands),
                    handle_close_stack_requests,
                    crate::archive::handle_close_tab_requests,
                    crate::window::spawn_requested_tab_layouts,
                )
                    .chain(),
            );

        app.world_mut()
            .spawn((bevy::window::Window::default(), PrimaryWindow));
        let space = app
            .world_mut()
            .spawn((
                crate::space::Space,
                crate::space::SpaceId("s1".to_string()),
                vmux_core::Active,
            ))
            .id();
        let worktree = tempfile::tempdir().unwrap();
        app.insert_resource(crate::settings::EffectiveStartupDir(Some((
            space,
            Some(worktree.path().to_path_buf()),
        ))));
        let tab_e = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Worktree".to_string(),
                    startup_dir: Some(worktree.path().to_string_lossy().into_owned()),
                },
                crate::tab::TabWorktree {
                    repo_root: worktree.path().to_string_lossy().into_owned(),
                    checkout_dir: worktree.path().to_string_lossy().into_owned(),
                    branch: "test".to_string(),
                    base_ref: "main".to_string(),
                },
                crate::tab::TabWorkspace {
                    project_dir: worktree.path().to_string_lossy().into_owned(),
                },
                crate::tab::TabDirDecided,
                crate::tab::TabWorktreeUnavailable {
                    message: "stale".to_string(),
                },
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab_e)))
            .id();
        let original_stack = app
            .world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));

        app.update();

        assert!(app.world().get_entity(tab_e).is_err());
        assert!(app.world().get_entity(original_stack).is_err());
        let replacement_tab = app
            .world_mut()
            .query_filtered::<Entity, With<Tab>>()
            .single(app.world())
            .unwrap();
        assert_ne!(replacement_tab, tab_e);
        assert_eq!(
            app.world().resource::<FocusedStack>().tab,
            Some(replacement_tab)
        );
        assert!(
            app.world()
                .get::<crate::tab::TabWorkspace>(replacement_tab)
                .is_none()
        );
        assert!(
            app.world()
                .get::<crate::tab::TabWorktree>(replacement_tab)
                .is_none()
        );
        assert!(
            app.world()
                .get::<crate::tab::TabDirDecided>(replacement_tab)
                .is_none()
        );
        assert!(
            app.world()
                .get::<crate::tab::TabWorktreeUnavailable>(replacement_tab)
                .is_none()
        );
        assert_eq!(
            app.world().get::<Tab>(replacement_tab).unwrap().startup_dir,
            None
        );
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        let [request] = opened.as_slice() else {
            panic!("the replacement tab opens exactly one page");
        };
        assert_eq!(request.url, vmux_core::EffectiveStartupUrl::START_PAGE);
        let PageOpenTarget::Stack(new_stack) = request.target else {
            panic!("the page is opened into a stack");
        };
        let new_pane = app
            .world()
            .get::<ChildOf>(new_stack)
            .map(Relationship::get)
            .unwrap();
        let split_root = app
            .world()
            .get::<ChildOf>(new_pane)
            .map(Relationship::get)
            .unwrap();
        assert_eq!(
            app.world()
                .get::<ChildOf>(split_root)
                .map(Relationship::get),
            Some(replacement_tab)
        );
    }

    #[test]
    fn closing_last_stack_in_tab_closes_the_tab_when_another_tab_exists() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<crate::tab::LastTabCloseAt>()
            .insert_resource(test_settings())
            .add_systems(
                Update,
                (
                    handle_stack_commands.in_set(WriteAppCommands),
                    handle_close_stack_requests,
                    crate::archive::handle_close_tab_requests,
                )
                    .chain(),
            );

        app.world_mut().spawn(PrimaryWindow);
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
    }

    #[test]
    fn closing_last_stack_in_active_rightmost_tab_activates_left_neighbor_not_first() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<crate::tab::LastTabCloseAt>()
            .insert_resource(test_settings())
            .add_systems(
                Update,
                (
                    handle_stack_commands.in_set(WriteAppCommands),
                    handle_close_stack_requests,
                    crate::archive::handle_close_tab_requests,
                )
                    .chain(),
            );

        app.world_mut().spawn(PrimaryWindow);
        let root = app.world_mut().spawn_empty().id();
        let make_tab = |app: &mut App, ts: i64| -> Entity {
            let tab = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(ts), ChildOf(root)))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, LastActivatedAt(ts), ChildOf(tab)))
                .id();
            app.world_mut()
                .spawn((Stack::default(), LastActivatedAt(ts), ChildOf(pane)));
            tab
        };
        let first = make_tab(&mut app, 1);
        let middle = make_tab(&mut app, 2);
        let active_rightmost = make_tab(&mut app, 3);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));

        app.update();

        assert!(app.world().get_entity(active_rightmost).is_err());
        let first_ts = app.world().get::<LastActivatedAt>(first).unwrap().0;
        let middle_ts = app.world().get::<LastActivatedAt>(middle).unwrap().0;
        assert_eq!(first_ts, 1, "first tab must not be re-activated");
        assert!(
            middle_ts > first_ts,
            "left neighbor (middle) must become most-recently-activated, not the first tab"
        );
    }

    #[test]
    fn closing_only_stack_in_split_pane_closes_pane() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<PendingCursorWarp>()
            .insert_resource(test_settings())
            .add_systems(
                Update,
                (
                    handle_stack_commands.in_set(WriteAppCommands),
                    handle_close_stack_requests,
                )
                    .chain(),
            );

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
    }

    #[test]
    fn closing_stack_in_three_way_split_keeps_split_and_does_not_respawn_startup() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<PendingCursorWarp>()
            .insert_resource(test_settings())
            .insert_resource(vmux_core::EffectiveStartupUrl(
                "vmux://agent/vibe/".to_string(),
            ))
            .add_systems(
                Update,
                (
                    handle_stack_commands.in_set(WriteAppCommands),
                    handle_close_stack_requests,
                )
                    .chain(),
            );

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
    fn empty_active_pane_opens_the_start_page_even_when_other_tabs_have_stacks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<PendingLaunch>()
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

        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        let [request] = opened.as_slice() else {
            panic!("an empty active pane opens exactly one page");
        };
        assert_eq!(request.url, vmux_core::EffectiveStartupUrl::START_PAGE);
        let PageOpenTarget::Stack(new_stack) = request.target else {
            panic!("the page is opened into a stack");
        };
        assert_eq!(
            app.world().get::<ChildOf>(new_stack).map(Relationship::get),
            Some(active_pane)
        );
    }

    #[test]
    fn empty_active_pane_does_not_open_a_page_when_tab_has_stacks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<PendingLaunch>()
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

        assert!(
            app.world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .next()
                .is_none(),
            "the tab already shows a page, so an empty sibling pane is not one to fill"
        );
    }

    #[test]
    fn a_pane_that_already_holds_a_stack_is_not_filled_again() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<PendingLaunch>()
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
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)));

        app.update();

        assert!(
            app.world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .next()
                .is_none()
        );
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
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<PendingCursorWarp>()
            .insert_resource(test_settings())
            .init_resource::<CollectedSpawns>()
            .add_systems(
                Update,
                (
                    handle_stack_commands.in_set(WriteAppCommands),
                    handle_close_stack_requests,
                    collect_spawn_requests,
                )
                    .chain(),
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
    fn closing_last_stack_requests_tab_replacement() {
        let mut app = build_app_with_collector();
        app.insert_resource(vmux_core::EffectiveStartupUrl(
            "https://startup.test".into(),
        ));
        let (tab, pane, original_stack) = build_hierarchy(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));

        app.update();

        assert!(app.world().get_entity(original_stack).is_ok());
        assert!(app.world().get_entity(tab).is_ok());

        let collected = app.world().resource::<CollectedSpawns>();
        assert!(collected.0.is_empty());
        let close_requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CloseTabRequest>>()
            .drain()
            .collect();
        assert_eq!(close_requests.len(), 1);
        assert_eq!(close_requests[0].tab, tab);
        assert_eq!(
            app.world()
                .get::<ChildOf>(original_stack)
                .map(Relationship::get),
            Some(pane)
        );
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
    fn open_in_new_stack_none_url_opens_the_start_page() {
        let mut app = build_app_with_collector();
        let (_tab, pane, _stack) = build_hierarchy(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewStack { url: None },
            )));

        app.update();

        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        let [request] = opened.as_slice() else {
            panic!("a new stack opens exactly one page");
        };
        assert_eq!(request.url, vmux_core::EffectiveStartupUrl::START_PAGE);
        let PageOpenTarget::Stack(opened_stack) = request.target else {
            panic!("a new stack is opened by entity");
        };
        assert_eq!(
            app.world()
                .get::<ChildOf>(opened_stack)
                .map(Relationship::get),
            Some(pane),
        );
    }

    #[test]
    fn in_new_stack_with_no_url_uses_startup_url() {
        let mut app = build_app_with_collector();
        app.insert_resource(vmux_core::EffectiveStartupUrl(
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
