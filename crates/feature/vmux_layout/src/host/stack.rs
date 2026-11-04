use crate::{
    active_pane::{ActivePaneScope, ActiveStack},
    host::swap::SiblingOrder,
    pane::{Pane, PaneHierarchy, PaneSplit, PaneStacks, PendingCursorWarp},
    presentation::{Projection, Retiring, Transition},
    tab::{CloseTabRequest, Tab},
};
use bevy::{
    ecs::{relationship::Relationship, system::SystemParam},
    prelude::*,
    window::{ClosingWindow, PrimaryWindow},
};
use moonshine_save::prelude::*;
#[cfg(test)]
use vmux_command::CommandDefinition;
use vmux_command::CommandInvocation;
#[cfg(test)]
use vmux_command::CommandManifest;
use vmux_ecs::page::{PageMetadataCatalog, PagePlacementCatalog};
use vmux_ecs::persistence::PersistenceAppExt;
pub use vmux_ecs::workspace::{ComputeFocusSet, StackCommandSet};
use vmux_ecs::{Active, PageMetadata, PageOpenRequest, PageOpenTarget};
#[cfg(test)]
use vmux_feature::AppFeatureExt;
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use super::{command::LayoutRequestSet, target::SiblingDirection};

pub use crate::active_pane::FocusedStack;

pub struct StackPlugin;

impl Plugin for StackPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.register_feature::<crate::Feature>();
        if !app.is_plugin_added::<vmux_command::CommandRuntimePlugin>() {
            app.add_plugins(vmux_command::CommandRuntimePlugin);
        }
        app.add_message::<OpenRequest>()
            .add_message::<CloseRequest>()
            .add_message::<FocusRequest>()
            .add_message::<MoveRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<crate::presentation::FocusChanged>()
            .add_systems(
                Startup,
                open_startup_url_if_no_stacks
                    .in_set(crate::LayoutStartupSet::Post)
                    .in_set(OpenStartupPageSet),
            )
            .register_persisted::<Stack>()
            .add_message::<CloseStackRequest>()
            .add_systems(
                Update,
                (open, close, focus, shift)
                    .chain()
                    .in_set(StackCommandSet)
                    .in_set(LayoutRequestSet::Handle),
            )
            .add_systems(
                Update,
                close_pending
                    .after(close)
                    .in_set(CloseStackSet)
                    .in_set(LayoutRequestSet::Handle),
            )
            .add_systems(
                Update,
                bevy::ecs::schedule::ApplyDeferred
                    .after(LayoutRequestSet::Handle)
                    .before(ComputeFocusSet),
            )
            .add_systems(
                Update,
                compute_focused
                    .in_set(ComputeFocusSet)
                    .after(LayoutRequestSet::Handle)
                    .after(crate::window::TabLayoutSpawnSet)
                    .after(crate::active::ActiveSystemSet::Descendants)
                    .after(crate::space::CurrentSpaceSet)
                    .before(vmux_ecs::PageOpenSet::ResolveTarget),
            )
            .add_systems(
                Update,
                open_startup_url_if_no_stacks
                    .in_set(OpenStartupPageSet)
                    .after(crate::window::WindowShellSet)
                    .after(crate::window::TabLayoutSpawnSet)
                    .before(vmux_ecs::PageOpenSet::ResolveTarget),
            );
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CloseStackSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct OpenStartupPageSet;

#[vmux_command::command(message)]
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub url: Option<String>,
    pub policy: OpenPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenPolicy {
    NewStack,
    ReplaceStartup,
    FocusExact,
    FocusReusable,
}

impl TryFrom<&CommandInvocation> for OpenRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "open_in_new_stack" => Ok(Self {
                url: invocation.argument("url"),
                policy: OpenPolicy::ReplaceStartup,
            }),
            _ => Err(()),
        }
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest;

impl TryFrom<&CommandInvocation> for CloseRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "stack_close").then_some(Self).ok_or(())
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusRequest(pub SiblingDirection);

impl TryFrom<&CommandInvocation> for FocusRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "stack_next" => Ok(Self(SiblingDirection::Next)),
            "stack_previous" => Ok(Self(SiblingDirection::Previous)),
            _ => Err(()),
        }
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveRequest(pub SiblingDirection);

impl TryFrom<&CommandInvocation> for MoveRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "stack_swap_prev" => Ok(Self(SiblingDirection::Previous)),
            "stack_swap_next" => Ok(Self(SiblingDirection::Next)),
            _ => Err(()),
        }
    }
}

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

    pub fn reset_pane(stack: Entity) -> Self {
        Self {
            stack,
            reason: CloseStackReason::ResetPane,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseStackReason {
    ByUser,
    Tidying,
    ResetPane,
}

fn close_pending(
    mut reader: MessageReader<CloseStackRequest>,
    mut closer: StackCloser,
    mut commands: Commands,
) {
    let mut requested = std::collections::HashSet::new();
    let mut requests = reader
        .read()
        .filter_map(|request| requested.insert(request.stack).then_some(*request))
        .collect::<Vec<_>>();
    requests.sort_by_key(|request| {
        closer
            .child_of
            .get(request.stack)
            .ok()
            .is_some_and(|parent| closer.active_stack(parent.parent()) == Some(request.stack))
    });
    let mut closed = std::collections::HashSet::new();
    for request in requests {
        close_stack(request, &requested, &closed, &mut closer, &mut commands);
        closed.insert(request.stack);
    }
}

#[derive(SystemParam)]
struct StackCloser<'w, 's> {
    all_children: Query<'w, 's, &'static Children>,
    panes: PaneHierarchy<'w, 's>,
    pane_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Pane>>,
    stack_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Stack>>,
    stacks: Query<'w, 's, Entity, With<Stack>>,
    tabs: Query<'w, 's, &'static Tab, Without<Retiring>>,
    retiring: Query<'w, 's, (), With<Retiring>>,
    transitions: Query<'w, 's, &'static Transition>,
    presentation: Projection<'w, 's>,
    pane_stacks: PaneStacks<'w, 's>,
    child_of: Query<'w, 's, &'static ChildOf>,
    splits: Query<'w, 's, &'static PaneSplit>,
    startup_urls: Query<'w, 's, &'static vmux_ecs::EffectiveStartupUrl, With<crate::space::Space>>,
    close_tab_requests: MessageWriter<'w, CloseTabRequest>,
    page_open_requests: MessageWriter<'w, PageOpenRequest>,
}

impl StackCloser<'_, '_> {
    fn tab(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        loop {
            if self.tabs.contains(current) {
                return Some(current);
            }
            current = self.child_of.get(current).ok()?.parent();
        }
    }

    fn active_stack(&self, pane: Entity) -> Option<Entity> {
        self.panes.children.get(pane).ok().and_then(|children| {
            LastActivatedAt::latest(
                children
                    .iter()
                    .filter(|entity| !self.retiring.contains(*entity))
                    .filter_map(|entity| self.stack_ts.get(entity).ok())
                    .map(|(entity, activated_at)| (entity, *activated_at)),
            )
        })
    }

    fn startup_url(&self, entity: Entity) -> String {
        let mut current = entity;
        loop {
            if let Ok(url) = self.startup_urls.get(current) {
                return url.0.clone();
            }
            let Ok(parent) = self.child_of.get(current) else {
                return String::new();
            };
            current = parent.parent();
        }
    }
}

fn close_stack(
    request: CloseStackRequest,
    requested: &std::collections::HashSet<Entity>,
    closed: &std::collections::HashSet<Entity>,
    closer: &mut StackCloser,
    commands: &mut Commands,
) {
    if closer.retiring.contains(request.stack) {
        return;
    }
    if let Some((incoming, mut transition)) = closer.presentation.retaining(request.stack) {
        commands.entity(request.stack).insert(Retiring);
        if !requested.contains(&incoming) {
            transition.retire(request.stack);
            commands.entity(incoming).insert(transition);
        }
        return;
    }
    let Some(tab) = closer.tab(request.stack) else {
        return;
    };
    let Ok(pane) = closer.child_of.get(request.stack).map(Relationship::get) else {
        return;
    };
    let Ok(children) = closer.panes.children.get(pane) else {
        return;
    };
    let was_active = closer.active_stack(pane) == Some(request.stack);
    if request.reason != CloseStackReason::ResetPane
        && was_active
        && let Some(visible) = closer.presentation.retained(request.stack)
        && visible != request.stack
        && !requested.contains(&visible)
        && !closed.contains(&visible)
        && closer
            .transitions
            .get(request.stack)
            .is_ok_and(Transition::cancelable)
    {
        if let Ok(transition) = closer.transitions.get(request.stack) {
            for entity in transition.retired() {
                commands.entity(*entity).try_remove::<Retiring>();
            }
        }
        commands.entity(visible).insert(LastActivatedAt::now());
        commands.entity(request.stack).try_despawn();
        return;
    }
    let stacks_in_pane: Vec<Entity> = children
        .iter()
        .filter(|&entity| {
            closer.stacks.contains(entity)
                && !closer.retiring.contains(entity)
                && !closed.contains(&entity)
        })
        .collect();

    if request.reason == CloseStackReason::ResetPane {
        close_last_stack_in_pane(pane, request.stack, tab, false, closer, commands);
        return;
    }

    if stacks_in_pane.len() <= 1 {
        if request.reason == CloseStackReason::Tidying {
            return;
        }
        close_last_stack_in_pane(
            pane,
            request.stack,
            tab,
            request.reason != CloseStackReason::ResetPane,
            closer,
            commands,
        );
        return;
    }

    if !was_active {
        commands.entity(request.stack).try_despawn();
        return;
    }
    let successor = LastActivatedAt::latest(
        stacks_in_pane
            .iter()
            .filter(|&&entity| entity != request.stack)
            .filter_map(|&entity| closer.stack_ts.get(entity).ok())
            .map(|(entity, activated_at)| (entity, *activated_at)),
    );
    if let Some(successor) = successor {
        let pending = closer.transitions.get(request.stack).ok();
        let visible = closer
            .presentation
            .retained(request.stack)
            .unwrap_or(request.stack);
        if successor == visible {
            if let Some(pending) = pending {
                for entity in pending.retired() {
                    commands.entity(*entity).try_remove::<Retiring>();
                }
            }
            commands.entity(successor).insert(LastActivatedAt::now());
            commands.entity(request.stack).try_despawn();
            return;
        }
        commands.entity(request.stack).insert(Retiring);
        commands.entity(successor).insert((
            LastActivatedAt::now(),
            Transition::retiring(visible, request.stack, pending),
        ));
    }
}

fn close_last_stack_in_pane(
    pane: Entity,
    stack: Entity,
    tab: Entity,
    close_tab: bool,
    closer: &mut StackCloser,
    commands: &mut Commands,
) {
    if close_tab && closes_tab(tab, stack, closer) {
        return;
    }

    let split_parent = match closer.child_of.get(pane).map(Relationship::get) {
        Ok(parent) if closer.splits.contains(parent) => Some(parent),
        _ => None,
    };
    let Some(parent) = split_parent else {
        let visible = closer.presentation.retained(stack).unwrap_or(stack);
        let pending = closer.transitions.get(stack).ok();
        commands.entity(stack).insert(Retiring);
        commands
            .entity(tab)
            .remove::<crate::page_context::PendingWorkspaceSelection>();
        let mut transition = Transition::retiring(visible, stack, pending);
        if !close_tab && let Ok(children) = closer.panes.children.get(pane) {
            for sibling in children.iter() {
                if sibling == stack || !closer.stacks.contains(sibling) {
                    continue;
                }
                commands.entity(sibling).insert(Retiring);
                transition.retire(sibling);
            }
        }
        transition = transition.reset_tab(tab);
        let replacement = commands
            .spawn((
                Stack::bundle(),
                LastActivatedAt::now(),
                transition,
                ChildOf(pane),
            ))
            .id();
        closer.page_open_requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(replacement),
            url: closer.startup_url(stack),
            request_id: None,
        });
        return;
    };

    commands.entity(stack).try_despawn();
    let Ok(siblings) = closer.panes.children.get(parent) else {
        return;
    };
    let pane_siblings: Vec<Entity> = siblings
        .iter()
        .filter(|&entity| {
            entity != pane
                && (closer.panes.leaves.contains(entity) || closer.splits.contains(entity))
        })
        .collect();

    if pane_siblings.len() >= 2 {
        commands.entity(pane).try_despawn();
        let new_active_pane = pane_siblings
            .iter()
            .copied()
            .max_by_key(|&entity| {
                closer
                    .pane_ts
                    .get(entity)
                    .map(|(_, time)| time.milliseconds())
                    .unwrap_or(0)
            })
            .unwrap_or(pane_siblings[0]);
        let focus_leaf = closer.panes.first_leaf(new_active_pane);
        commands.entity(focus_leaf).insert(LastActivatedAt::now());
        if let Some(next) = first_stack_to_activate(focus_leaf, closer) {
            commands.entity(next).insert(LastActivatedAt::now());
        }
        return;
    }

    let Some(sibling) = pane_siblings.into_iter().next() else {
        return;
    };
    let sibling_children: Vec<Entity> = closer
        .panes
        .children
        .get(sibling)
        .map(|children| children.iter().collect())
        .unwrap_or_default();

    for &child in &sibling_children {
        commands.entity(child).insert(ChildOf(parent));
    }

    let new_active_pane;
    if closer.splits.contains(sibling) {
        let sibling_direction = closer
            .splits
            .get(sibling)
            .map(|split| split.direction)
            .unwrap_or_default();
        new_active_pane = closer.panes.first_leaf(sibling);
        commands.entity(sibling).remove::<ChildOf>();
        commands.entity(sibling).try_despawn();
        commands.entity(parent).insert(PaneSplit {
            direction: sibling_direction,
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
        commands.entity(sibling).try_despawn();
    }

    commands.entity(pane).try_despawn();
    commands
        .entity(new_active_pane)
        .insert(LastActivatedAt::now());
    let next = first_stack_to_activate(new_active_pane, closer).or_else(|| {
        sibling_children
            .iter()
            .copied()
            .find(|&entity| closer.stacks.contains(entity))
    });
    if let Some(next) = next {
        commands.entity(next).insert(LastActivatedAt::now());
    }
}

fn first_stack_to_activate(pane: Entity, closer: &StackCloser) -> Option<Entity> {
    closer
        .active_stack(pane)
        .or_else(|| closer.pane_stacks.first(pane))
}

fn closes_tab(tab: Entity, stack: Entity, closer: &mut StackCloser) -> bool {
    if entity_tree_contains_stack_other_than(
        tab,
        stack,
        &closer.all_children,
        &closer.stacks,
        &closer.retiring,
    ) {
        return false;
    }
    let Ok(parent) = closer.child_of.get(tab) else {
        return false;
    };
    let Ok(siblings) = closer.all_children.get(parent.parent()) else {
        return false;
    };
    if siblings
        .iter()
        .filter(|entity| closer.tabs.contains(*entity))
        .count()
        <= 1
    {
        return false;
    }
    closer.close_tab_requests.write(CloseTabRequest { tab });
    true
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::stack"]
#[require(Save, crate::presentation::Layer, crate::page_slot::Current)]
pub struct Stack {
    pub scroll_x: f32,
    pub scroll_y: f32,
}

impl Stack {
    pub fn bundle() -> impl Bundle {
        (
            Self::default(),
            crate::page_slot::Current::default(),
            PageMetadata::default(),
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
}

#[derive(SystemParam)]
pub struct ActiveTabParam<'w, 's> {
    tabs: Query<'w, 's, (Entity, &'static LastActivatedAt), (With<Tab>, Without<Retiring>)>,
    active_tabs: Query<'w, 's, Entity, (With<Tab>, With<Active>, Without<Retiring>)>,
    focused_space: crate::space::FocusedSpace<'w, 's>,
    spaces: Query<'w, 's, (), With<crate::space::Space>>,
    child_of: Query<'w, 's, &'static ChildOf>,
}

impl ActiveTabParam<'_, '_> {
    pub fn get(&self) -> Option<Entity> {
        let Some(active_space) = self.focused_space.entity() else {
            if !self.spaces.is_empty() {
                return None;
            }
            return self.active_tabs.iter().next().or_else(|| {
                LastActivatedAt::latest(
                    self.tabs
                        .iter()
                        .map(|(entity, activated_at)| (entity, *activated_at)),
                )
            });
        };
        if let Some(active) = self.active_tabs.iter().find(|&tab| {
            self.child_of
                .get(tab)
                .is_ok_and(|parent| parent.parent() == active_space)
        }) {
            return Some(active);
        }
        LastActivatedAt::latest(
            self.tabs
                .iter()
                .filter(|(tab, _)| {
                    self.child_of
                        .get(*tab)
                        .is_ok_and(|parent| parent.parent() == active_space)
                })
                .map(|(entity, activated_at)| (entity, *activated_at)),
        )
    }
}

#[derive(SystemParam)]
pub struct LayoutFocus<'w, 's> {
    all_children: Query<'w, 's, &'static Children>,
    leaf_panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    active_panes: Query<'w, 's, (), (With<Pane>, With<Active>)>,
    split_panes: Query<'w, 's, (), With<PaneSplit>>,
    pane_activity: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Pane>>,
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    active_stacks: Query<'w, 's, (), (With<Stack>, With<Active>, Without<Retiring>)>,
    stack_activity:
        Query<'w, 's, (Entity, &'static LastActivatedAt), (With<Stack>, Without<Retiring>)>,
    stack_metadata: Query<'w, 's, &'static PageMetadata, With<Stack>>,
    parents: Query<'w, 's, &'static ChildOf>,
    tabs: Query<'w, 's, (), (With<Tab>, Without<Retiring>)>,
}

impl LayoutFocus<'_, '_> {
    fn is_only_startup_stack(&self, stack: Entity, startup_url: &str) -> bool {
        if self.stack_activity.iter().count() != 1 {
            return false;
        }
        self.stack_metadata.get(stack).is_ok_and(|metadata| {
            metadata.url.trim_end_matches('/') == startup_url.trim_end_matches('/')
        })
    }

    fn collect_leaf_panes(&self, root: Entity, result: &mut Vec<Entity>) {
        if self.leaf_panes.contains(root) {
            result.push(root);
        }
        if let Ok(children) = self.all_children.get(root) {
            for child in children.iter() {
                self.collect_leaf_panes(child, result);
            }
        }
    }

    pub fn is_leaf(&self, entity: Entity) -> bool {
        self.leaf_panes.contains(entity)
    }

    pub fn leaf_count(&self) -> usize {
        self.leaf_panes.iter().count()
    }

    pub fn leaves(&self, root: Entity) -> Vec<Entity> {
        let mut panes = Vec::new();
        self.collect_leaf_panes(root, &mut panes);
        panes
    }

    pub fn pane(&self, tab: Entity) -> Option<Entity> {
        let mut panes = Vec::new();
        self.collect_leaf_panes(tab, &mut panes);
        if let Some(active) = panes
            .iter()
            .copied()
            .find(|pane| self.pane_is_active(*pane, tab))
        {
            return Some(active);
        }
        LastActivatedAt::latest(
            panes
                .iter()
                .filter_map(|&entity| self.pane_activity.get(entity).ok())
                .map(|(entity, activated_at)| (entity, *activated_at)),
        )
    }

    pub fn stack(&self, pane: Entity) -> Option<Entity> {
        self.pane_children.get(pane).ok().and_then(|children| {
            if let Some(active) = children
                .iter()
                .find(|entity| self.active_stacks.contains(*entity))
            {
                return Some(active);
            }
            LastActivatedAt::latest(
                children
                    .iter()
                    .filter_map(|entity| self.stack_activity.get(entity).ok())
                    .map(|(entity, activated_at)| (entity, *activated_at)),
            )
        })
    }

    fn pane_is_active(&self, pane: Entity, tab: Entity) -> bool {
        let mut current = pane;
        for _ in 0..32 {
            let Ok(parent) = self.parents.get(current) else {
                return false;
            };
            if parent.parent() == tab {
                return true;
            }
            if self.split_panes.contains(parent.parent()) && !self.active_panes.contains(current) {
                return false;
            }
            current = parent.parent();
        }
        false
    }

    pub fn resolve(
        &self,
        active_tab: Option<Entity>,
    ) -> (Option<Entity>, Option<Entity>, Option<Entity>) {
        let pane = active_tab.and_then(|tab| self.pane(tab));
        let stack = pane.and_then(|pane| self.stack(pane));
        (active_tab, pane, stack)
    }

    pub fn tab_of(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        for _ in 0..32 {
            if self.tabs.contains(current) {
                return Some(current);
            }
            current = self.parents.get(current).ok()?.parent();
        }
        None
    }

    fn focus_stack(&self, stack: Entity, commands: &mut Commands) {
        if let Ok(parent) = self.parents.get(stack) {
            commands
                .entity(parent.parent())
                .insert(LastActivatedAt::now());
        }
        commands.entity(stack).insert(LastActivatedAt::now());
        if let Some(tab) = self.tab_of(stack) {
            commands.entity(tab).insert(LastActivatedAt::now());
        }
    }
}

fn compute_focused(
    mut scopes: Query<(&ActivePaneScope, &mut ActiveStack)>,
    active_tab_param: ActiveTabParam,
    focus: LayoutFocus,
    mut changes: MessageWriter<crate::presentation::FocusChanged>,
    mut commands: Commands,
) {
    let tab = active_tab_param.get();
    let (_, pane, stack) = focus.resolve(tab);
    let next = ActiveStack { tab, pane, stack };
    for (scope, mut active) in &mut scopes {
        if *scope == ActivePaneScope::Local {
            if *active != next {
                changes.write(crate::presentation::FocusChanged {
                    previous: *active,
                    current: next,
                });
                *active = next;
            }
            return;
        }
    }
    changes.write(crate::presentation::FocusChanged {
        previous: ActiveStack::default(),
        current: next,
    });
    commands.spawn(next.local_bundle());
}

fn open(
    mut reader: MessageReader<OpenRequest>,
    active_tab_param: ActiveTabParam,
    focus: LayoutFocus,
    focused_space: crate::space::FocusedSpace,
    target: StackOpenTarget,
    metadata: PageMetadataCatalog,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let mut startup_reused = false;
    let mut pending = Vec::<(String, Entity)>::new();
    for request in reader.read() {
        let (active_tab, active_pane, active_stack) = focus.resolve(active_tab_param.get());
        let Some(pane) = active_pane else {
            continue;
        };
        let startup_url = focused_space.resolved_startup_url();
        let url = request
            .url
            .clone()
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| startup_url.clone());
        let reusable = active_tab.and_then(|tab| match request.policy {
            OpenPolicy::FocusExact => target
                .exact(&focus, tab, &url)
                .or_else(|| target.pending_exact(&pending, &url)),
            OpenPolicy::FocusReusable => target
                .reusable(&focus, tab, &url)
                .or_else(|| target.pending_reusable(&pending, &url)),
            OpenPolicy::NewStack | OpenPolicy::ReplaceStartup => None,
        });
        if let Some(stack) = reusable {
            target.retain_current_until_presented(&focus, stack, &mut commands);
            focus.focus_stack(stack, &mut commands);
            if target.navigates(stack, &url) {
                page_open_requests.write(PageOpenRequest {
                    target: PageOpenTarget::Stack(stack),
                    url: url.clone(),
                    request_id: None,
                });
            }
            continue;
        }
        let stack = active_stack.filter(|stack| {
            if request.policy != OpenPolicy::ReplaceStartup
                || startup_reused
                || !target.is_startup(*stack, &startup_url)
            {
                return false;
            }
            startup_reused = true;
            true
        });
        let stack = stack.unwrap_or_else(|| {
            let stack = commands
                .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
                .id();
            if let Some(previous) = active_stack.map(|stack| target.reveal_previous(stack)) {
                commands.entity(stack).insert(Transition::retain(previous));
            }
            stack
        });
        commands
            .entity(stack)
            .insert(metadata.metadata_for(&url, &url));
        page_open_requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url: url.clone(),
            request_id: None,
        });
        pending.push((url, stack));
    }
}

#[derive(SystemParam)]
struct StackOpenTarget<'w, 's> {
    metadata: Query<'w, 's, &'static PageMetadata, (With<Stack>, Without<Retiring>)>,
    currents: Query<'w, 's, &'static crate::page_slot::Current, (With<Stack>, Without<Retiring>)>,
    placements: PagePlacementCatalog<'w, 's>,
    presentation: Projection<'w, 's>,
    parents: Query<'w, 's, &'static ChildOf>,
}

impl StackOpenTarget<'_, '_> {
    fn reveal_previous(&self, stack: Entity) -> Entity {
        self.presentation.retained(stack).unwrap_or(stack)
    }

    fn exact(&self, focus: &LayoutFocus, tab: Entity, url: &str) -> Option<Entity> {
        self.find(focus, tab, |existing| Self::same_url(url, existing))
    }

    fn retain_current_until_presented(
        &self,
        focus: &LayoutFocus,
        stack: Entity,
        commands: &mut Commands,
    ) {
        let Ok(parent) = self.parents.get(stack) else {
            return;
        };
        let Some(current) = focus.stack(parent.parent()) else {
            return;
        };
        if current == stack {
            return;
        }
        let visible = self.reveal_previous(current);
        commands.entity(stack).insert(Transition::retain(visible));
    }

    fn reusable(&self, focus: &LayoutFocus, tab: Entity, url: &str) -> Option<Entity> {
        self.exact(focus, tab, url)
            .or_else(|| self.find(focus, tab, |existing| self.placements.reuses(url, existing)))
    }

    fn find(
        &self,
        focus: &LayoutFocus,
        tab: Entity,
        matches: impl Fn(&str) -> bool,
    ) -> Option<Entity> {
        for pane in focus.leaves(tab) {
            let Ok(children) = focus.pane_children.get(pane) else {
                continue;
            };
            for stack in children.iter() {
                if self.urls(stack).into_iter().flatten().any(&matches) {
                    return Some(stack);
                }
            }
        }
        None
    }

    fn pending_exact(&self, pending: &[(String, Entity)], url: &str) -> Option<Entity> {
        pending
            .iter()
            .find(|(pending_url, _)| Self::same_url(url, pending_url))
            .map(|(_, stack)| *stack)
    }

    fn pending_reusable(&self, pending: &[(String, Entity)], url: &str) -> Option<Entity> {
        self.pending_exact(pending, url).or_else(|| {
            pending
                .iter()
                .find(|(pending_url, _)| self.placements.reuses(url, pending_url))
                .map(|(_, stack)| *stack)
        })
    }

    fn navigates(&self, stack: Entity, url: &str) -> bool {
        let Some(existing) = self.url(stack) else {
            return true;
        };
        !(Self::same_url(url, existing)
            || self.placements.group_entrypoint(url) && self.placements.same_group(url, existing))
    }

    fn same_url(left: &str, right: &str) -> bool {
        match (
            vmux_api::VmuxRoute::canonical(left),
            vmux_api::VmuxRoute::canonical(right),
        ) {
            (Some(left), Some(right)) => left == right,
            _ => left.trim_end_matches('/') == right.trim_end_matches('/'),
        }
    }

    fn url(&self, stack: Entity) -> Option<&str> {
        self.metadata
            .get(stack)
            .ok()
            .map(|metadata| metadata.url.as_str())
            .filter(|url| !url.is_empty())
            .or_else(|| {
                self.currents
                    .get(stack)
                    .ok()
                    .map(|current| current.metadata().url.as_str())
                    .filter(|url| !url.is_empty())
            })
    }

    fn urls(&self, stack: Entity) -> [Option<&str>; 2] {
        let intent = self
            .metadata
            .get(stack)
            .ok()
            .map(|metadata| metadata.url.as_str())
            .filter(|url| !url.is_empty());
        let current = self
            .currents
            .get(stack)
            .ok()
            .map(|current| current.metadata().url.as_str())
            .filter(|url| !url.is_empty());
        [intent, current]
    }

    fn is_startup(&self, stack: Entity, startup_url: &str) -> bool {
        let Some(url) = self.url(stack) else {
            return false;
        };
        match (
            vmux_api::VmuxRoute::parse(url),
            vmux_api::VmuxRoute::parse(startup_url),
        ) {
            (Some(current), Some(startup)) => current.same_page(&startup),
            _ => url.trim_end_matches('/') == startup_url.trim_end_matches('/'),
        }
    }
}

fn close(
    mut reader: MessageReader<CloseRequest>,
    active_tab_param: ActiveTabParam,
    focus: LayoutFocus,
    focused_space: crate::space::FocusedSpace,
    mut requests: MessageWriter<CloseStackRequest>,
    surfaces: crate::hosted_page::StackSurface,
    mut commands: Commands,
) {
    for _ in reader.read() {
        let (_, _, active_stack) = focus.resolve(active_tab_param.get());
        let Some(active_stack) = active_stack else {
            continue;
        };
        let startup =
            focus.is_only_startup_stack(active_stack, &focused_space.resolved_startup_url());
        if let Some(page) = surfaces.current(active_stack) {
            commands.queue(move |world: &mut World| {
                let request = vmux_ecs::page::CloseItemRequest::new(page);
                world.trigger(request.clone());
                if !request.handled() && !startup {
                    world.write_message(CloseStackRequest::by_user(active_stack));
                }
            });
        } else if !startup {
            requests.write(CloseStackRequest::by_user(active_stack));
        }
    }
}

fn focus(
    mut reader: MessageReader<FocusRequest>,
    active_tab_param: ActiveTabParam,
    focus: LayoutFocus,
    stack_q: Query<Entity, With<Stack>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let (active_tab, active_pane, active_stack) = focus.resolve(active_tab_param.get());
        let Some(active_tab) = active_tab else {
            continue;
        };
        let tab_panes = focus.leaves(active_tab);
        let mut stacks = Vec::new();
        for &pane in &tab_panes {
            if let Ok(children) = focus.pane_children.get(pane) {
                for child in children.iter() {
                    if stack_q.contains(child) {
                        stacks.push((pane, child));
                    }
                }
            }
        }
        if stacks.len() < 2 {
            continue;
        }
        let Some(current) = stacks
            .iter()
            .position(|&(_, stack)| Some(stack) == active_stack)
        else {
            continue;
        };
        let delta = if request.0 == SiblingDirection::Next {
            1
        } else {
            -1
        };
        let index = (current as i32 + delta).rem_euclid(stacks.len() as i32) as usize;
        let (target_pane, target_stack) = stacks[index];
        commands.entity(target_stack).insert(LastActivatedAt::now());
        if active_pane != Some(target_pane) {
            commands
                .entity(target_pane)
                .insert((LastActivatedAt::now(), PendingCursorWarp));
        }
    }
}

fn shift(
    mut reader: MessageReader<MoveRequest>,
    active_tab_param: ActiveTabParam,
    focus: LayoutFocus,
    stack_q: Query<Entity, With<Stack>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let (_, active_pane, active_stack) = focus.resolve(active_tab_param.get());
        let Some(pane) = active_pane else {
            continue;
        };
        let Some(stack) = active_stack else {
            continue;
        };
        let Ok(children) = focus.pane_children.get(pane) else {
            continue;
        };
        let kind_positions: Vec<usize> = children
            .iter()
            .enumerate()
            .filter(|(_, entity)| stack_q.contains(*entity))
            .map(|(index, _)| index)
            .collect();
        let Some(active_index) = SiblingOrder::index(stack, children, &kind_positions) else {
            continue;
        };
        let pair = if request.0 == SiblingDirection::Previous {
            SiblingOrder::previous(active_index)
        } else {
            SiblingOrder::next(active_index, kind_positions.len())
        };
        if let Some((left, right)) = pair
            && let Some(order) = SiblingOrder::swapped(pane, children, &kind_positions, left, right)
        {
            commands.queue(order);
        }
    }
}

fn entity_tree_contains_stack_other_than(
    entity: Entity,
    ignored_stack: Entity,
    all_children: &Query<&Children>,
    stack_q: &Query<Entity, With<Stack>>,
    retiring: &Query<(), With<Retiring>>,
) -> bool {
    (stack_q.contains(entity) && entity != ignored_stack && !retiring.contains(entity))
        || all_children.get(entity).is_ok_and(|children| {
            children.iter().any(|child| {
                entity_tree_contains_stack_other_than(
                    child,
                    ignored_stack,
                    all_children,
                    stack_q,
                    retiring,
                )
            })
        })
}

fn open_startup_url_if_no_stacks(
    active_tab_param: ActiveTabParam,
    focus: LayoutFocus,
    stack_q: Query<Entity, With<Stack>>,
    closing_primary: Query<(), (With<PrimaryWindow>, With<ClosingWindow>)>,
    focused_space: crate::space::FocusedSpace,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    if !closing_primary.is_empty() {
        return;
    }
    let Some(active_tab) = active_tab_param.get() else {
        return;
    };
    let (_, active_pane, _) = focus.resolve(Some(active_tab));
    if entity_tree_contains_stack(active_tab, &focus.all_children, &stack_q) {
        return;
    }
    let Some(pane) = active_pane else {
        return;
    };
    let stack = commands
        .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    page_open_requests.write(PageOpenRequest {
        target: PageOpenTarget::Stack(stack),
        url: focused_space.resolved_startup_url(),
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
    use crate::LauncherDismissRequest;
    use crate::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use bevy::ecs::relationship::Relationship;

    use bevy::ecs::system::RunSystemOnce;

    #[derive(Resource)]
    struct InnerCloses {
        pages: Vec<Entity>,
        accept: bool,
    }

    fn claim_inner(event: On<vmux_ecs::page::CloseItemRequest>, mut closed: ResMut<InnerCloses>) {
        closed.pages.push(event.entity);
        if closed.accept {
            event.handle();
        }
    }

    #[test]
    fn startup_page_offers_inner_close_before_protecting_the_last_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StackPlugin))
            .add_message::<LauncherDismissRequest>()
            .add_message::<CloseTabRequest>()
            .insert_resource(test_settings())
            .insert_resource(InnerCloses {
                pages: Vec::new(),
                accept: true,
            })
            .add_observer(claim_inner);
        let url = "file:///repo/README.md";
        let (_, _, stack) = build_hierarchy(&mut app, url);
        let metadata = PageMetadata {
            url: url.to_string(),
            ..Default::default()
        };
        let page = app
            .world_mut()
            .spawn((crate::cef::Browser, metadata.clone(), ChildOf(stack)))
            .id();
        app.world_mut().entity_mut(stack).insert((
            metadata.clone(),
            crate::page_slot::Current::new(Some(page), metadata),
        ));
        app.world_mut().write_message(CloseRequest);
        app.update();
        assert_eq!(app.world().resource::<InnerCloses>().pages, [page]);
        assert!(app.world().get_entity(stack).is_ok());
        app.world_mut().resource_mut::<InnerCloses>().accept = false;
        app.world_mut().write_message(CloseRequest);
        app.update();
        assert_eq!(app.world().resource::<InnerCloses>().pages, [page, page]);
        assert!(app.world().get_entity(stack).is_ok());
    }

    #[test]
    fn stack_mcp_definitions_are_the_dispatchable_command_set() {
        let definitions = CommandManifest::for_feature::<crate::Feature>().into_vec();
        let tools = definitions
            .iter()
            .filter_map(CommandDefinition::agent_tool)
            .filter(|tool| {
                let invocation = CommandInvocation::new(Entity::PLACEHOLDER, &tool.name);
                OpenRequest::try_from(&invocation).is_ok()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["open_in_new_stack"],
        );
        for tool in tools {
            let invocation = CommandInvocation::new(Entity::PLACEHOLDER, tool.name);
            assert!(OpenRequest::try_from(&invocation).is_ok());
        }
    }

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
            .add_message::<LauncherDismissRequest>()
            .add_systems(Update, close_pending);
        app
    }

    fn focused(app: &mut App) -> ActiveStack {
        let world = app.world_mut();
        let mut query = world.query::<&ActiveStack>();
        *query.single(world).unwrap()
    }

    #[test]
    fn a_stack_does_not_invent_an_activation_timestamp() {
        let mut world = World::new();
        let stack = world.spawn(Stack::default()).id();

        assert!(world.get::<LastActivatedAt>(stack).is_none());
    }

    #[test]
    fn close_stack_request_despawns_target_keeps_siblings() {
        let mut app = close_request_app();

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let s1 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
            .id();
        let s2 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(2), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::tidying(s1));
        app.update();

        assert!(app.world().get_entity(s1).is_err(), "target despawned");
        assert!(app.world().get_entity(s2).is_ok(), "sibling kept");
    }

    #[test]
    fn duplicate_close_requests_despawn_a_stack_once() {
        let mut app = close_request_app();

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let target = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
            .id();
        let sibling = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(2), ChildOf(pane)))
            .id();

        let mut requests = app
            .world_mut()
            .resource_mut::<Messages<CloseStackRequest>>();
        requests.write(CloseStackRequest::by_user(target));
        requests.write(CloseStackRequest::by_user(target));
        app.update();

        assert!(app.world().get_entity(target).is_err());
        assert!(app.world().get_entity(sibling).is_ok());
    }

    #[test]
    fn closing_every_stack_in_one_frame_creates_one_replacement() {
        let mut app = close_request_app();
        let space = app.world_mut().spawn(crate::space::CurrentSpace).id();
        let tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::now(), ChildOf(space)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let first = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
            .id();
        let second = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(2), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(second));
        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(first));
        app.update();

        assert!(app.world().get_entity(first).is_err());
        assert!(app.world().get::<Retiring>(second).is_some());
        let replacements = app
            .world()
            .get::<Children>(pane)
            .unwrap()
            .iter()
            .filter(|entity| {
                *entity != first && *entity != second && app.world().get::<Stack>(*entity).is_some()
            })
            .collect::<Vec<_>>();
        assert_eq!(replacements.len(), 1);
        let transition = app.world().get::<Transition>(replacements[0]).unwrap();
        assert_eq!(transition.outgoing(), second);
        assert!(app.world().get_entity(transition.outgoing()).is_ok());
    }

    #[test]
    fn resetting_a_pane_retains_every_old_stack_until_replacement() {
        let mut app = close_request_app();
        let space = app.world_mut().spawn(crate::space::CurrentSpace).id();
        let tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::now(), ChildOf(space)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let outgoing = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
            .id();
        let incoming = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(2),
                Transition::retain(outgoing),
                ChildOf(pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::reset_pane(incoming));
        app.update();

        let replacement = app
            .world()
            .get::<Children>(pane)
            .unwrap()
            .iter()
            .find(|entity| *entity != outgoing && *entity != incoming)
            .unwrap();
        let transition = app.world().get::<Transition>(replacement).unwrap();
        assert_eq!(transition.outgoing(), outgoing);
        assert!(transition.retired().contains(&outgoing));
        assert!(transition.retired().contains(&incoming));
        assert!(app.world().get::<Retiring>(outgoing).is_some());
        assert!(app.world().get::<Retiring>(incoming).is_some());
    }

    #[test]
    fn closing_a_retained_stack_defers_retirement_to_its_owner() {
        let mut app = close_request_app();
        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let outgoing = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
            .id();
        let incoming = app
            .world_mut()
            .spawn((
                Stack::default(),
                Active,
                LastActivatedAt::new(2),
                Transition::retain(outgoing),
                ChildOf(pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(outgoing));
        app.update();

        assert!(app.world().get_entity(outgoing).is_ok());
        assert!(app.world().get::<Retiring>(outgoing).is_some());
        assert_eq!(
            app.world().get::<Transition>(incoming).unwrap().retired(),
            [outgoing]
        );
    }

    #[test]
    fn close_request_ignores_an_already_retiring_stack() {
        let mut app = close_request_app();

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let retiring = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(2),
                Retiring,
                ChildOf(pane),
            ))
            .id();
        let survivor = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(retiring));
        app.update();

        assert!(app.world().get_entity(retiring).is_ok());
        assert!(app.world().get_entity(survivor).is_ok());
        assert_eq!(app.world().get::<Children>(pane).unwrap().iter().count(), 2);
        assert!(
            app.world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn close_request_ignores_a_stack_inside_a_retiring_tab() {
        let mut app = close_request_app();

        let space = app.world_mut().spawn_empty().id();
        let retiring_tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::new(2), Retiring, ChildOf(space)))
            .id();
        app.world_mut()
            .spawn((Tab, LastActivatedAt::new(1), ChildOf(space)));
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(2), ChildOf(retiring_tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(2), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(stack));
        app.update();

        assert!(app.world().get_entity(stack).is_ok());
        assert!(
            app.world_mut()
                .resource_mut::<Messages<CloseTabRequest>>()
                .drain()
                .next()
                .is_none()
        );
        assert!(
            app.world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn closing_background_tabs_last_stack_targets_its_own_tab() {
        let mut app = close_request_app();

        let space = app.world_mut().spawn(crate::space::CurrentSpace).id();
        let active_tab = app
            .world_mut()
            .spawn((Tab, Active, LastActivatedAt::new(2), ChildOf(space)))
            .id();
        let active_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(2), ChildOf(active_tab)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            LastActivatedAt::new(2),
            ChildOf(active_pane),
        ));
        let background_tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::new(1), ChildOf(space)))
            .id();
        let background_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(1), ChildOf(background_tab)))
            .id();
        let background_stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(1),
                ChildOf(background_pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(background_stack));
        app.update();

        let close_requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CloseTabRequest>>()
            .drain()
            .collect();
        assert_eq!(close_requests.len(), 1);
        assert_eq!(close_requests[0].tab, background_tab);
        assert!(app.world().get::<Retiring>(background_stack).is_none());
        assert!(
            app.world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn tidying_the_last_stack_in_a_pane_leaves_the_pane_alone() {
        let mut app = close_request_app();

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let only = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
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

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let first = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
            .id();
        let middle = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(2), ChildOf(pane)))
            .id();
        let active = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(3), ChildOf(pane)))
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
            app.world()
                .get::<LastActivatedAt>(active)
                .unwrap()
                .milliseconds(),
            3,
            "closing an inactive stack must not re-stamp the active one"
        );
        assert_eq!(
            app.world()
                .get::<LastActivatedAt>(first)
                .unwrap()
                .milliseconds(),
            1
        );
    }

    #[test]
    fn closing_the_active_stack_keeps_it_until_the_successor_is_presented() {
        let mut app = close_request_app();

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let oldest = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
            .id();
        let runner_up = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(2), ChildOf(pane)))
            .id();
        let active = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(3), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(active));
        app.update();

        assert!(app.world().get_entity(active).is_ok());
        assert!(app.world().get::<Retiring>(active).is_some());
        assert_eq!(
            app.world().get::<Transition>(runner_up),
            Some(&Transition::retiring(active, active, None))
        );
        let oldest_ts = app
            .world()
            .get::<LastActivatedAt>(oldest)
            .unwrap()
            .milliseconds();
        let runner_up_ts = app
            .world()
            .get::<LastActivatedAt>(runner_up)
            .unwrap()
            .milliseconds();
        assert_eq!(oldest_ts, 1, "the oldest stack must not be activated");
        assert!(
            runner_up_ts > oldest_ts,
            "the most recently used survivor takes over"
        );
    }

    #[test]
    fn closing_an_unpresented_close_successor_preserves_every_close_obligation() {
        let mut app = close_request_app();

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let visible = app
            .world_mut()
            .spawn((
                Stack::default(),
                crate::page_slot::Current::default(),
                LastActivatedAt::new(1),
                Retiring,
                ChildOf(pane),
            ))
            .id();
        let pending = app
            .world_mut()
            .spawn((
                Stack::default(),
                crate::page_slot::Current::default(),
                LastActivatedAt::new(2),
                Transition::retiring(visible, visible, None),
                ChildOf(pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(pending));
        app.update();

        assert!(app.world().get_entity(visible).is_ok());
        assert!(app.world().get::<Retiring>(visible).is_some());
        assert!(app.world().get::<Retiring>(pending).is_some());
        let replacement = app
            .world()
            .get::<Children>(pane)
            .unwrap()
            .iter()
            .find(|entity| *entity != visible && *entity != pending)
            .unwrap();
        let transition = app.world().get::<Transition>(replacement).unwrap();
        assert_eq!(transition.outgoing(), visible);
        assert!(transition.retired().contains(&visible));
        assert!(transition.retired().contains(&pending));
    }

    #[test]
    fn closing_an_unpresented_focus_successor_restores_its_predecessor() {
        let mut app = close_request_app();
        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let visible = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)))
            .id();
        let pending = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(2),
                Transition::retain(visible),
                ChildOf(pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(pending));
        app.update();

        assert!(app.world().get_entity(pending).is_err());
        assert!(app.world().get_entity(visible).is_ok());
        assert!(app.world().get::<Retiring>(visible).is_none());
    }

    #[test]
    fn focused_stack_not_rewritten_when_focus_is_stable() {
        #[derive(Resource, Default)]
        struct ChangeLog(Vec<bool>);

        fn probe(focused: FocusedStack, mut log: ResMut<ChangeLog>) {
            log.0.push(focused.is_changed());
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::presentation::FocusChanged>()
            .init_resource::<ChangeLog>()
            .add_systems(Update, (compute_focused, probe).chain());
        app.world_mut().spawn(ActiveStack::default().local_bundle());

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
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
            focused(&mut app).stack,
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
    fn closing_last_stack_resets_workspace_without_hiding_the_current_page() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StackPlugin,
            crate::archive::ArchivePlugin,
            crate::presentation::PresentationPlugin,
            crate::window::LayoutSpawnPlugin,
        ))
        .add_message::<PageOpenRequest>()
        .add_message::<LauncherDismissRequest>()
        .insert_resource(test_settings());
        app.world_mut().spawn(ActiveStack::default().local_bundle());

        app.world_mut()
            .spawn((bevy::window::Window::default(), PrimaryWindow));
        let space = app
            .world_mut()
            .spawn((
                crate::space::Space::bundle(vmux_api::Id::<crate::space::Space>::new("s1")),
                vmux_ecs::Active,
            ))
            .id();
        let worktree = tempfile::tempdir().unwrap();
        app.world_mut().entity_mut(space).insert((
            crate::space::EffectiveStartupDir(Some(worktree.path().to_path_buf())),
            vmux_ecs::EffectiveStartupUrl("vmux://start/".to_string()),
        ));
        let tab_e = app
            .world_mut()
            .spawn((
                Tab::named("Worktree").with_startup_dir(worktree.path()),
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
                Active,
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
            .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

        app.update();

        assert!(app.world().get_entity(tab_e).is_ok());
        assert!(app.world().get_entity(original_stack).is_ok());
        let replacement_tab = app
            .world_mut()
            .query_filtered::<Entity, With<Tab>>()
            .single(app.world())
            .unwrap();
        assert_eq!(replacement_tab, tab_e);
        assert_eq!(focused(&mut app).tab, Some(replacement_tab));
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        let [request] = opened.as_slice() else {
            panic!("the replacement tab opens exactly one page");
        };
        assert_eq!(request.url, "vmux://start/");
        let PageOpenTarget::Stack(new_stack) = request.target else {
            panic!("the page is opened into a stack");
        };
        assert_eq!(
            app.world().get::<Transition>(new_stack),
            Some(
                &Transition::retiring(original_stack, original_stack, None)
                    .reset_tab(replacement_tab)
            )
        );
        app.world_mut().spawn((
            crate::cef::Browser,
            vmux_ecs::page::PageReady,
            vmux_ecs::PagePresented,
            crate::hosted_page::SurfacePresentationReady,
            ChildOf(new_stack),
        ));
        for _ in 0..3 {
            app.update();
        }

        assert!(app.world().get_entity(original_stack).is_err());
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
            app.world()
                .get::<crate::tab::StartupDir>(replacement_tab)
                .unwrap()
                .0,
            None
        );
        let new_pane = app
            .world()
            .get::<ChildOf>(new_stack)
            .map(Relationship::get)
            .unwrap();
        assert_eq!(
            app.world().get::<ChildOf>(new_pane).map(Relationship::get),
            Some(replacement_tab)
        );
    }

    #[test]
    fn closing_last_stack_in_tab_closes_the_tab_when_another_tab_exists() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StackPlugin, crate::archive::ArchivePlugin))
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<LauncherDismissRequest>()
            .insert_resource(test_settings());

        app.world_mut().spawn(PrimaryWindow);
        let root = app.world_mut().spawn_empty().id();
        let remaining_tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::new(1), ChildOf(root)))
            .id();
        let remaining_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(1), ChildOf(remaining_tab)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            LastActivatedAt::new(1),
            ChildOf(remaining_pane),
        ));

        let closing_tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::new(2), ChildOf(root)))
            .id();
        let closing_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(2), ChildOf(closing_tab)))
            .id();
        let closing_stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(2),
                ChildOf(closing_pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

        app.update();

        assert!(app.world().get_entity(closing_tab).is_err());
        assert!(app.world().get_entity(closing_stack).is_err());
        assert!(app.world().get_entity(remaining_tab).is_ok());
        assert!(
            app.world()
                .get::<LastActivatedAt>(remaining_tab)
                .unwrap()
                .milliseconds()
                > 1
        );
    }

    #[test]
    fn closing_last_stack_in_active_rightmost_tab_activates_left_neighbor_not_first() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StackPlugin, crate::archive::ArchivePlugin))
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<LauncherDismissRequest>()
            .insert_resource(test_settings());

        app.world_mut().spawn(PrimaryWindow);
        let root = app.world_mut().spawn_empty().id();
        let make_tab = |app: &mut App, ts: i64| -> Entity {
            let tab = app
                .world_mut()
                .spawn((Tab, LastActivatedAt::new(ts), ChildOf(root)))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, LastActivatedAt::new(ts), ChildOf(tab)))
                .id();
            app.world_mut()
                .spawn((Stack::default(), LastActivatedAt::new(ts), ChildOf(pane)));
            tab
        };
        let first = make_tab(&mut app, 1);
        let middle = make_tab(&mut app, 2);
        let active_rightmost = make_tab(&mut app, 3);

        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

        app.update();

        assert!(app.world().get_entity(active_rightmost).is_err());
        let first_ts = app
            .world()
            .get::<LastActivatedAt>(first)
            .unwrap()
            .milliseconds();
        let middle_ts = app
            .world()
            .get::<LastActivatedAt>(middle)
            .unwrap()
            .milliseconds();
        assert_eq!(first_ts, 1, "first tab must not be re-activated");
        assert!(
            middle_ts > first_ts,
            "left neighbor (middle) must become most-recently-activated, not the first tab"
        );
    }

    #[test]
    fn closing_only_stack_in_split_pane_closes_pane() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CloseRequest>()
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<LauncherDismissRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, (close, close_pending).chain());

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let split = app
            .world_mut()
            .spawn((
                Pane::split_bundle(crate::pane::PaneSplitDirection::Row),
                ChildOf(tab),
            ))
            .id();
        let active_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(2), ChildOf(split)))
            .id();
        let other_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(1), ChildOf(split)))
            .id();
        let original_stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(2),
                ChildOf(active_pane),
            ))
            .id();
        let other_stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(1),
                ChildOf(other_pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

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
        app.add_plugins(MinimalPlugins)
            .add_message::<CloseRequest>()
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<LauncherDismissRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, (close, close_pending).chain());

        let tab = app.world_mut().spawn((Tab, LastActivatedAt::now())).id();
        let split = app
            .world_mut()
            .spawn((
                Pane::split_bundle(crate::pane::PaneSplitDirection::Row),
                ChildOf(tab),
            ))
            .id();
        let active_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(3), ChildOf(split)))
            .id();
        let p2 = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(2), ChildOf(split)))
            .id();
        let p3 = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(1), ChildOf(split)))
            .id();
        let active_stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(3),
                ChildOf(active_pane),
            ))
            .id();
        let s2 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(2), ChildOf(p2)))
            .id();
        let s3 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(p3)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);
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
            .add_message::<LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .add_systems(Update, open_startup_url_if_no_stacks);

        let active_space = app
            .world_mut()
            .spawn((
                crate::space::Space::bundle(vmux_api::Id::<crate::space::Space>::new("active")),
                crate::space::CurrentSpace,
                Active,
                vmux_ecs::EffectiveStartupUrl("vmux://start/".to_string()),
            ))
            .id();
        let old_space = app
            .world_mut()
            .spawn(crate::space::Space::bundle(vmux_api::Id::<
                crate::space::Space,
            >::new("old")))
            .id();

        let old_tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::new(1), ChildOf(old_space)))
            .id();
        let old_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(1), ChildOf(old_tab)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(old_pane)));

        let active_tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::new(2), ChildOf(active_space)))
            .id();
        let active_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(2), ChildOf(active_tab)))
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
        assert_eq!(request.url, "vmux://start/");
        let PageOpenTarget::Stack(new_stack) = request.target else {
            panic!("the page is opened into a stack");
        };
        assert_eq!(
            app.world().get::<ChildOf>(new_stack).map(Relationship::get),
            Some(active_pane)
        );
    }

    #[test]
    fn focused_space_without_a_tab_does_not_fill_another_spaces_pane() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .add_systems(Update, open_startup_url_if_no_stacks);

        app.world_mut().spawn((
            crate::space::Space,
            crate::space::CurrentSpace,
            Active,
            vmux_ecs::EffectiveStartupUrl("vmux://start/".to_string()),
        ));
        let other_space = app.world_mut().spawn(crate::space::Space).id();
        let other_tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::new(1), ChildOf(other_space)))
            .id();
        let other_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(1), ChildOf(other_tab)))
            .id();
        let existing = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(1),
                ChildOf(other_pane),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(existing).is_ok());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Stack>>()
                .iter(app.world())
                .count(),
            1
        );
        assert!(
            app.world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn empty_active_pane_does_not_open_a_page_when_tab_has_stacks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .add_systems(Update, open_startup_url_if_no_stacks);

        let tab_e = app.world_mut().spawn((Tab, LastActivatedAt::new(1))).id();
        let pane_with_stack = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(1), ChildOf(tab_e)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            LastActivatedAt::new(1),
            ChildOf(pane_with_stack),
        ));
        app.world_mut()
            .spawn((Pane, LastActivatedAt::new(2), ChildOf(tab_e)));

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
            .add_message::<LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .add_systems(Update, open_startup_url_if_no_stacks);

        let tab_e = app.world_mut().spawn((Tab, LastActivatedAt::new(1))).id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::new(1), ChildOf(tab_e)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(1), ChildOf(pane)));

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
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenRequest>()
            .add_message::<CloseRequest>()
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<LauncherDismissRequest>()
            .insert_resource(test_settings())
            .init_resource::<CollectedSpawns>()
            .add_systems(
                Update,
                (open, close, close_pending, collect_spawn_requests).chain(),
            );
        app
    }

    fn build_hierarchy(app: &mut App, startup_url: &str) -> (Entity, Entity, Entity) {
        let space = app
            .world_mut()
            .spawn((
                crate::space::Space::bundle(vmux_api::Id::<crate::space::Space>::new("space")),
                crate::space::CurrentSpace,
                Active,
                vmux_ecs::EffectiveStartupUrl(startup_url.to_string()),
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::now(), ChildOf(space)))
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
    fn closing_last_stack_keeps_it_visible_until_startup_presents() {
        let mut app = build_app_with_collector();
        let (tab, pane, original_stack) = build_hierarchy(&mut app, "");

        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

        app.update();

        assert!(app.world().get_entity(original_stack).is_ok());
        assert!(app.world().get_entity(tab).is_ok());

        let collected = app.world().resource::<CollectedSpawns>();
        let [request] = collected.0.as_slice() else {
            panic!("one replacement page is opened");
        };
        let PageOpenTarget::Stack(replacement) = request.target else {
            panic!("replacement opens in a stack");
        };
        assert_ne!(replacement, original_stack);
        assert_eq!(
            app.world().get::<Transition>(replacement),
            Some(&Transition::retiring(original_stack, original_stack, None).reset_tab(tab))
        );
        assert!(app.world().get::<Retiring>(original_stack).is_some());
        let close_requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CloseTabRequest>>()
            .drain()
            .collect();
        assert!(close_requests.is_empty());
        assert_eq!(
            app.world()
                .get::<ChildOf>(original_stack)
                .map(Relationship::get),
            Some(pane)
        );
    }

    #[test]
    fn closing_a_background_spaces_last_stack_uses_its_own_startup_url() {
        let mut app = build_app_with_collector();
        let background_space = app
            .world_mut()
            .spawn((
                crate::space::Space,
                vmux_ecs::EffectiveStartupUrl("vmux://projects/background".to_string()),
            ))
            .id();
        let background_tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::now(), ChildOf(background_space)))
            .id();
        let background_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(background_tab)))
            .id();
        let background_stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::now(),
                ChildOf(background_pane),
            ))
            .id();
        app.world_mut().spawn((
            crate::space::Space,
            crate::space::CurrentSpace,
            Active,
            vmux_ecs::EffectiveStartupUrl("vmux://sessions/".to_string()),
        ));

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(background_stack));
        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        let [request] = collected.0.as_slice() else {
            panic!("one replacement page opens");
        };
        assert_eq!(request.url, "vmux://projects/background");
    }

    #[test]
    fn closing_only_start_page_keeps_the_existing_stack() {
        let mut app = build_app_with_collector();
        let (_tab, _pane, stack) = build_hierarchy(&mut app, "vmux://start/");
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            title: "Start".to_string(),
            url: "vmux://start/".to_string(),
            ..Default::default()
        });
        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

        app.update();

        assert!(app.world().get_entity(stack).is_ok());
        assert!(app.world().resource::<CollectedSpawns>().0.is_empty());
        assert!(
            app.world_mut()
                .resource_mut::<Messages<CloseTabRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn open_in_new_stack_with_explicit_url() {
        let mut app = build_app_with_collector();
        let (_tab, pane, previous) = build_hierarchy(&mut app, "");

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("https://example.com".into()),
                policy: OpenPolicy::ReplaceStartup,
            });

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
                assert_eq!(
                    app.world().get::<Transition>(*stack),
                    Some(&Transition::retain(previous)),
                );
            }
            other => panic!("expected PageOpenRequest, got {other:?}"),
        }
    }

    #[test]
    fn open_focuses_an_existing_page_in_the_active_tab() {
        let mut app = build_app_with_collector();
        let (_tab, pane, existing) = build_hierarchy(&mut app, "");
        app.world_mut().entity_mut(existing).insert((
            LastActivatedAt::new(1),
            PageMetadata {
                url: "vmux://services/".into(),
                ..Default::default()
            },
        ));
        let active = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(2),
                PageMetadata {
                    url: "vmux://projects/".into(),
                    ..Default::default()
                },
                ChildOf(pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("vmux://services/".into()),
                policy: OpenPolicy::FocusExact,
            });
        app.update();

        assert!(app.world().resource::<CollectedSpawns>().0.is_empty());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Stack>>()
                .iter(app.world())
                .count(),
            2
        );
        assert!(
            app.world()
                .get::<LastActivatedAt>(existing)
                .unwrap()
                .milliseconds()
                > app
                    .world()
                    .get::<LastActivatedAt>(active)
                    .unwrap()
                    .milliseconds()
        );
        assert_eq!(
            app.world().get::<Transition>(existing),
            Some(&Transition::retain(active))
        );
    }

    #[test]
    fn open_does_not_reuse_a_retiring_stack() {
        let mut app = build_app_with_collector();
        let (_tab, pane, retiring) = build_hierarchy(&mut app, "");
        app.world_mut().entity_mut(retiring).insert((
            Retiring,
            LastActivatedAt::new(1),
            PageMetadata {
                url: "vmux://services/".into(),
                ..Default::default()
            },
        ));
        app.world_mut().spawn((
            Stack::default(),
            LastActivatedAt::new(2),
            PageMetadata {
                url: "vmux://projects/".into(),
                ..Default::default()
            },
            ChildOf(pane),
        ));

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("vmux://services/".into()),
                policy: OpenPolicy::FocusExact,
            });
        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        let [request] = collected.0.as_slice() else {
            panic!("one replacement page is opened");
        };
        let PageOpenTarget::Stack(opened) = request.target else {
            panic!("replacement opens in a stack");
        };
        assert_ne!(opened, retiring);
        assert_eq!(
            app.world()
                .get::<LastActivatedAt>(retiring)
                .unwrap()
                .milliseconds(),
            1
        );
    }

    #[test]
    fn group_entrypoint_focuses_existing_page_without_navigation() {
        let mut app = build_app_with_collector();
        app.world_mut().spawn(vmux_ecs::page::PageManifest {
            route: "git://",
            url: "git://",
            asset_host: "git",
            owns_subtree: true,
            title: "Git",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: Some(vmux_api::BuiltinIcon::GitBranch),
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: vmux_ecs::page::PagePlacement {
                group: "git",
                reuse: vmux_ecs::page::PageReuse::Group,
                ..vmux_ecs::page::PagePlacement::DEFAULT
            },
        });
        let (_tab, pane, existing) = build_hierarchy(&mut app, "");
        app.world_mut().entity_mut(existing).insert((
            LastActivatedAt::new(1),
            PageMetadata {
                url: "git://repo-a?branch=feature".into(),
                ..Default::default()
            },
        ));
        let active = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(2),
                PageMetadata {
                    url: "vmux://projects/".into(),
                    ..Default::default()
                },
                ChildOf(pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("git://".into()),
                policy: OpenPolicy::FocusReusable,
            });
        app.update();

        assert!(app.world().resource::<CollectedSpawns>().0.is_empty());
        assert!(
            app.world()
                .get::<LastActivatedAt>(existing)
                .unwrap()
                .milliseconds()
                > app
                    .world()
                    .get::<LastActivatedAt>(active)
                    .unwrap()
                    .milliseconds()
        );
    }

    #[test]
    fn exact_focus_does_not_reuse_another_page_in_the_same_group() {
        let mut app = build_app_with_collector();
        app.world_mut().spawn(vmux_ecs::page::PageManifest {
            route: "git://",
            url: "git://",
            asset_host: "git",
            owns_subtree: true,
            title: "Git",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: Some(vmux_api::BuiltinIcon::GitBranch),
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: vmux_ecs::page::PagePlacement {
                group: "git",
                reuse: vmux_ecs::page::PageReuse::Group,
                ..vmux_ecs::page::PagePlacement::DEFAULT
            },
        });
        let (_tab, pane, existing) = build_hierarchy(&mut app, "");
        app.world_mut().entity_mut(existing).insert(PageMetadata {
            url: "git://repo-a".into(),
            ..Default::default()
        });

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("git://repo-b".into()),
                policy: OpenPolicy::FocusExact,
            });
        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        let [request] = collected.0.as_slice() else {
            panic!("one page is opened");
        };
        let PageOpenTarget::Stack(opened) = request.target else {
            panic!("page opens in a stack");
        };
        assert_ne!(opened, existing);
        assert_eq!(
            app.world().get::<ChildOf>(opened).map(Relationship::get),
            Some(pane)
        );
    }

    #[test]
    fn exact_group_page_is_preferred_over_another_group_member() {
        let mut app = build_app_with_collector();
        app.world_mut().spawn(vmux_ecs::page::PageManifest {
            route: "git://",
            url: "git://",
            asset_host: "git",
            owns_subtree: true,
            title: "Git",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: Some(vmux_api::BuiltinIcon::GitBranch),
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: vmux_ecs::page::PagePlacement {
                group: "git",
                reuse: vmux_ecs::page::PageReuse::Group,
                ..vmux_ecs::page::PagePlacement::DEFAULT
            },
        });
        let (_tab, pane, first) = build_hierarchy(&mut app, "");
        app.world_mut().entity_mut(first).insert((
            LastActivatedAt::new(1),
            PageMetadata {
                url: "git://repo-a".into(),
                ..Default::default()
            },
        ));
        let exact = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(2),
                PageMetadata {
                    url: "git://repo-b".into(),
                    ..Default::default()
                },
                ChildOf(pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("git://repo-b".into()),
                policy: OpenPolicy::FocusReusable,
            });
        app.update();

        assert!(app.world().resource::<CollectedSpawns>().0.is_empty());
        assert!(
            app.world()
                .get::<LastActivatedAt>(exact)
                .unwrap()
                .milliseconds()
                > app
                    .world()
                    .get::<LastActivatedAt>(first)
                    .unwrap()
                    .milliseconds()
        );
    }

    #[test]
    fn open_reuses_the_active_startup_stack() {
        let mut app = build_app_with_collector();
        let (_tab, _pane, stack) = build_hierarchy(&mut app, "vmux://start/");
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: "vmux://start/".into(),
            ..Default::default()
        });

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("git://".into()),
                policy: OpenPolicy::ReplaceStartup,
            });

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        let [request] = collected.0.as_slice() else {
            panic!("one page is opened");
        };
        assert!(matches!(request.target, PageOpenTarget::Stack(target) if target == stack));
        assert_eq!(request.url, "git://");
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Stack>>()
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn exact_open_does_not_reuse_a_retiring_stack() {
        let mut app = build_app_with_collector();
        let (_tab, pane, active) = build_hierarchy(&mut app, "vmux://start/");
        app.world_mut().entity_mut(active).insert(PageMetadata {
            url: "vmux://projects/".into(),
            ..Default::default()
        });
        let retiring = app
            .world_mut()
            .spawn((
                Stack::default(),
                Retiring,
                LastActivatedAt::new(2),
                PageMetadata {
                    url: "vmux://sessions/".into(),
                    ..Default::default()
                },
                ChildOf(pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("vmux://sessions/".into()),
                policy: OpenPolicy::FocusExact,
            });
        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        let [request] = collected.0.as_slice() else {
            panic!("one page is opened");
        };
        let PageOpenTarget::Stack(opened) = request.target else {
            panic!("page opens in a stack");
        };
        assert_ne!(opened, retiring);
        assert!(app.world().get::<Retiring>(opened).is_none());
    }

    #[test]
    fn forced_new_stack_does_not_reuse_the_active_startup_stack() {
        let mut app = build_app_with_collector();
        let (_tab, pane, startup) = build_hierarchy(&mut app, "vmux://start/");
        app.world_mut().entity_mut(startup).insert(PageMetadata {
            url: "vmux://start/".into(),
            ..Default::default()
        });

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("git://".into()),
                policy: OpenPolicy::NewStack,
            });

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        let [request] = collected.0.as_slice() else {
            panic!("one page is opened");
        };
        let PageOpenTarget::Stack(opened) = request.target else {
            panic!("page opens in a stack");
        };
        assert_ne!(opened, startup);
        assert_eq!(
            app.world().get::<ChildOf>(opened).map(Relationship::get),
            Some(pane)
        );
        assert_eq!(
            app.world().get::<Transition>(opened),
            Some(&Transition::retain(startup))
        );
    }

    #[test]
    fn exact_focus_does_not_replace_the_active_startup_stack() {
        let mut app = build_app_with_collector();
        let (_tab, _pane, startup) = build_hierarchy(&mut app, "vmux://start/");
        app.world_mut().entity_mut(startup).insert(PageMetadata {
            url: "vmux://start/".into(),
            ..Default::default()
        });

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("git://".into()),
                policy: OpenPolicy::FocusExact,
            });
        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        let [request] = collected.0.as_slice() else {
            panic!("one page is opened");
        };
        assert!(matches!(request.target, PageOpenTarget::Stack(opened) if opened != startup));
    }

    #[test]
    fn open_reuses_the_active_startup_stack_once_per_batch() {
        let mut app = build_app_with_collector();
        let (_tab, _pane, stack) = build_hierarchy(&mut app, "vmux://start/");
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: "vmux://start/".into(),
            ..Default::default()
        });
        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("git://".into()),
                policy: OpenPolicy::ReplaceStartup,
            });
        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("vmux://sessions/".into()),
                policy: OpenPolicy::ReplaceStartup,
            });

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        let [first, second] = collected.0.as_slice() else {
            panic!("two pages are opened");
        };
        assert!(matches!(first.target, PageOpenTarget::Stack(target) if target == stack));
        assert!(matches!(second.target, PageOpenTarget::Stack(target) if target != stack));
    }

    #[test]
    fn open_in_new_stack_none_url_opens_the_start_page() {
        let mut app = build_app_with_collector();
        let (_tab, pane, _stack) = build_hierarchy(&mut app, "vmux://start/");

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: None,
                policy: OpenPolicy::ReplaceStartup,
            });

        app.update();

        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        let [request] = opened.as_slice() else {
            panic!("a new stack opens exactly one page");
        };
        assert_eq!(request.url, "vmux://start/");
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
        let (_tab, _pane, _stack) = build_hierarchy(&mut app, "https://startup.test");

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: None,
                policy: OpenPolicy::ReplaceStartup,
            });

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        assert_eq!(collected.0[0].url, "https://startup.test");
    }

    #[test]
    fn active_tab_param_picks_active_space_tab_not_global_max() {
        let mut app = App::new();
        let main = app.world_mut().spawn(crate::window::Main).id();
        let space_a = app
            .world_mut()
            .spawn((
                crate::space::Space,
                vmux_api::Id::<crate::space::Space>::new(
                    "active_tab_param_picks_active_space_tab_not_global_max-0",
                ),
                ChildOf(main),
            ))
            .id();
        let _tab_a = app
            .world_mut()
            .spawn((Tab, Active, LastActivatedAt::new(100), ChildOf(space_a)))
            .id();
        let space_b = app
            .world_mut()
            .spawn((
                crate::space::Space,
                vmux_api::Id::<crate::space::Space>::new(
                    "active_tab_param_picks_active_space_tab_not_global_max-1",
                ),
                Active,
                ChildOf(main),
            ))
            .id();
        let tab_b = app
            .world_mut()
            .spawn((Tab, Active, LastActivatedAt::new(1), ChildOf(space_b)))
            .id();
        app.world_mut()
            .entity_mut(space_b)
            .insert(crate::space::CurrentSpace);

        let got = app
            .world_mut()
            .run_system_once(|param: ActiveTabParam| param.get())
            .unwrap();

        assert_eq!(got, Some(tab_b));
    }

    #[test]
    fn active_tab_param_repairs_within_current_space() {
        let mut app = App::new();
        let main = app.world_mut().spawn(crate::window::Main).id();
        let space = app
            .world_mut()
            .spawn((
                crate::space::Space,
                vmux_api::Id::<crate::space::Space>::new(
                    "active_tab_param_repairs_within_current_space-0",
                ),
                crate::space::CurrentSpace,
                Active,
                ChildOf(main),
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab, LastActivatedAt::new(5), ChildOf(space)))
            .id();

        let got = app
            .world_mut()
            .run_system_once(|param: ActiveTabParam| param.get())
            .unwrap();

        assert_eq!(got, Some(tab));
    }

    #[test]
    fn active_tab_param_never_falls_back_to_another_space() {
        let mut app = App::new();
        let main = app.world_mut().spawn(crate::window::Main).id();
        app.world_mut().spawn((
            crate::space::Space,
            crate::space::CurrentSpace,
            ChildOf(main),
        ));
        let other = app
            .world_mut()
            .spawn((crate::space::Space, ChildOf(main)))
            .id();
        app.world_mut()
            .spawn((Tab, LastActivatedAt::new(100), ChildOf(other)));

        let got = app
            .world_mut()
            .run_system_once(|param: ActiveTabParam| param.get())
            .unwrap();

        assert_eq!(got, None);
    }

    #[test]
    fn layout_focus_prefers_the_active_stack_over_newer_mru() {
        let mut app = App::new();
        let pane = app.world_mut().spawn(Pane).id();
        let active = app
            .world_mut()
            .spawn((
                Stack::default(),
                Active,
                LastActivatedAt::new(1),
                ChildOf(pane),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::new(100), ChildOf(pane)));

        let focused = app
            .world_mut()
            .run_system_once(move |focus: LayoutFocus| focus.stack(pane))
            .unwrap();

        assert_eq!(focused, Some(active));
    }

    #[test]
    fn layout_focus_walks_the_active_pane_branch() {
        let mut app = App::new();
        let tab = app.world_mut().spawn(Tab).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: Default::default(),
                },
                ChildOf(tab),
            ))
            .id();
        let active = app
            .world_mut()
            .spawn((Pane, Active, LastActivatedAt::new(1), ChildOf(root)))
            .id();
        app.world_mut()
            .spawn((Pane, LastActivatedAt::new(100), ChildOf(root)));

        let focused = app
            .world_mut()
            .run_system_once(move |focus: LayoutFocus| focus.pane(tab))
            .unwrap();

        assert_eq!(focused, Some(active));
    }
}
