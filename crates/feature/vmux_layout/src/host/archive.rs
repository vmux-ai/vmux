use std::collections::HashSet;
#[cfg(test)]
use std::path::PathBuf;

use bevy::ecs::system::{Commands, SystemParam};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
#[cfg(test)]
use bevy_cef::prelude::HostWindow;
use vmux_command::{CommandDispatch, CommandRuntimePlugin};
#[cfg(test)]
use vmux_ecs::PageOpenTarget;
#[cfg(test)]
use vmux_ecs::store::PageRestore;
use vmux_ecs::store::StoreAppExt;
use vmux_ecs::terminal::TerminalLaunch;
use vmux_ecs::{
    Active, ArchivedPage, ArchivedPagePosition, ArchivedTabPage, CreatedAt, PageArchiveRequest,
    PageMetadata, PageOpenRequest, PaneStep, SplitAxis, TabCommandSet, UnixMillis, WorkspaceKind,
};
#[cfg(test)]
use vmux_feature::manifest::FeaturePlugin;
use vmux_history::LastActivatedAt;

use super::command::LayoutRequestSet;
use crate::pane::{Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection};
use crate::presentation::{Projection, Retiring, Transition};
use crate::settings::LayoutSettings;
use crate::space::{CurrentSpace, Space, SpaceHierarchy, SpaceId};
#[cfg(test)]
use crate::stack::CloseRequest;
use crate::stack::{
    ActiveTabParam, CloseStackReason, CloseStackRequest, CloseStackSet, Stack, StackCommandSet,
};
use crate::tab::{CloseTabRequest, Tab, TabClosed, TabHierarchy};
use crate::{TabLayoutSpawnContent, TabLayoutSpawnRequest};

mod restore;
mod topology;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct ReopenClosedPage;

#[vmux_command::command]
struct StackReopenBinding;

fn issue_reopen_closed_page(
    trigger: On<CommandDispatch>,
    registered: Query<(), With<StackReopenBinding>>,
    mut requests: MessageWriter<ReopenClosedPage>,
) {
    if registered.contains(trigger.event().command()) {
        requests.write(ReopenClosedPage);
    }
}

pub struct ArchivePlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct CaptureEarlySet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct CaptureLateSet;

#[derive(Component)]
struct PendingTabClose {
    retire: bool,
}

impl Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.register_stored::<ArchivedPage>()
            .register_stored::<ArchivedPagePosition>()
            .register_stored::<ArchivedTabPage>()
            .register_type::<PaneStep>()
            .register_type::<SplitAxis>()
            .register_type::<Vec<PaneStep>>()
            .add_plugins((topology::TopologyPlugin, restore::RestorePlugin))
            .add_message::<ReopenClosedPage>()
            .add_message::<PageArchiveRequest>()
            .add_message::<CloseStackRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<LayoutSettings>()
            .add_observer(issue_reopen_closed_page)
            .add_observer(capture_page)
            .add_systems(
                Update,
                maintain
                    .after(topology::TopologySet)
                    .before(restore::RestoreAdmissionSet),
            )
            .add_systems(
                Update,
                (capture, finish_tabs)
                    .chain()
                    .in_set(CaptureLateSet)
                    .after(handle_close_tab_requests)
                    .in_set(LayoutRequestSet::Handle),
            )
            .add_systems(
                Update,
                capture_archived_pages
                    .after(stack_close)
                    .in_set(CaptureEarlySet)
                    .before(CloseStackSet),
            )
            .add_systems(
                Update,
                (
                    stack_close.after(StackCommandSet).before(CloseStackSet),
                    handle_reopen_closed_page.in_set(restore::RestoreAdmissionSet),
                    handle_close_tab_requests
                        .after(TabCommandSet)
                        .after(StackCommandSet)
                        .after(CloseStackSet),
                )
                    .in_set(LayoutRequestSet::Handle),
            );
    }
}

fn stack_close(
    mut reader: MessageReader<CloseStackRequest>,
    layout: TabArchiveLayout,
    mut writer: MessageWriter<PageArchiveRequest>,
) {
    let mut seen = HashSet::new();
    for request in reader.read() {
        if request.reason != CloseStackReason::ByUser || !seen.insert(request.stack) {
            continue;
        }
        if let Some(archive) = layout.request(request.stack) {
            writer.write(archive);
        }
    }
}

fn capture_archived_pages(mut reader: MessageReader<PageArchiveRequest>, mut commands: Commands) {
    for request in reader.read() {
        if let Some(bundle) = PageCaptureIntent::new(request, UnixMillis::now().0, false) {
            let operation = commands
                .spawn((
                    vmux_ecs::Operation::bundle("archive.capture-page").unwrap(),
                    vmux_ecs::OperationRetained,
                    bundle,
                ))
                .id();
            commands.trigger(CapturePage { operation });
        }
    }
}

#[derive(Component, Clone)]
struct PageCaptureIntent {
    source: Option<Entity>,
    workspace: Option<WorkspaceKind>,
}

#[derive(Component)]
struct TabCaptureIntent {
    tab: Entity,
}

#[derive(EntityEvent)]
struct CapturePage {
    #[event_target]
    operation: Entity,
}

fn capture_page(
    request: On<CapturePage>,
    intents: Query<(&PageCaptureIntent, &ArchivedPage, &ArchivedPagePosition)>,
    mut commands: Commands,
) {
    let Ok((intent, page, position)) = intents.get(request.operation) else {
        return;
    };
    spawn_page_capture(intent, page, position, &mut commands);
    commands.entity(request.operation).despawn();
}

fn capture(world: &mut World, tabs: &mut QueryState<(Entity, &TabCaptureIntent)>) {
    let pending = tabs
        .iter(world)
        .map(|(operation, intent)| (operation, intent.tab))
        .collect::<Vec<_>>();
    for (operation, tab) in pending {
        capture_tab(world, tab);
        world.despawn(operation);
        world.flush();
    }
}

fn finish_tabs(pending: Query<(Entity, &PendingTabClose)>, mut commands: Commands) {
    for (tab, pending) in &pending {
        if pending.retire {
            commands
                .entity(tab)
                .insert(Retiring)
                .remove::<PendingTabClose>();
        } else {
            commands.entity(tab).despawn();
        }
    }
}

fn spawn_page_capture(
    intent: &PageCaptureIntent,
    page: &ArchivedPage,
    position: &ArchivedPagePosition,
    commands: &mut Commands,
) {
    let root = commands
        .spawn((
            topology::Root,
            topology::Id(uuid::Uuid::new_v4().to_string()),
            Name::new(page.title.clone()),
            CreatedAt(page.closed_at),
            topology::Origin {
                space_id: page.space_id.clone(),
                tab_index: page.tab_index,
            },
            topology::Materialized,
        ))
        .id();
    let mut parent = root;
    let validity = topology::Root::validate_position(position);
    if validity.is_ok() {
        for (depth, step) in position.pane_path.iter().enumerate() {
            let (order, weight) = if depth == 0 {
                (0, 1.0)
            } else {
                let previous = &position.pane_path[depth - 1];
                (
                    previous.child_index as u32,
                    previous.flex_weights[previous.child_index],
                )
            };
            let split = commands
                .spawn((
                    topology::Id(step.split_id.clone()),
                    topology::Split::new(step.axis),
                    vmux_ecs::Order(order),
                    topology::Weight(weight),
                    ChildOf(parent),
                ))
                .id();
            for (index, weight) in step.flex_weights.iter().enumerate() {
                if index != step.child_index {
                    commands.spawn((
                        topology::Slot,
                        vmux_ecs::Order(index as u32),
                        topology::Weight(*weight),
                        ChildOf(split),
                    ));
                }
            }
            parent = split;
        }
        let (order, weight) = position.pane_path.last().map_or((0, 1.0), |step| {
            (step.child_index as u32, step.flex_weights[step.child_index])
        });
        let id = if position.leaf_pane_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            position.leaf_pane_id.clone()
        };
        parent = commands
            .spawn((
                topology::Leaf,
                topology::Id(id),
                vmux_ecs::Order(order),
                topology::Weight(weight),
                ChildOf(parent),
            ))
            .id();
    }
    let archive = commands
        .spawn((
            page.clone(),
            ChildOf(parent),
            vmux_ecs::Order(position.stack_index as u32),
            Active,
            topology::Materialized,
        ))
        .id();
    if let Err(error) = validity {
        commands
            .entity(root)
            .insert(topology::InvalidTopology(error.clone()));
        commands
            .entity(archive)
            .insert((topology::InvalidTopology(error), position.clone()));
    }
    if let Some(workspace) = intent.workspace {
        commands.entity(archive).insert(workspace);
    }
    if let Some(source) = intent.source {
        commands.trigger(vmux_ecs::page::ArchiveCaptureRequest { archive, source });
    }
}

impl PageCaptureIntent {
    fn new(
        request: &PageArchiveRequest,
        closed_at: i64,
        include_empty: bool,
    ) -> Option<(Self, ArchivedPage, ArchivedPagePosition)> {
        if request.url.is_empty() && !include_empty {
            return None;
        }
        Some((
            Self {
                source: request.source,
                workspace: request.workspace,
            },
            ArchivedPage {
                url: request.url.clone(),
                title: request.title.clone(),
                space_id: request.space_id.clone(),
                closed_at,
                launch: request.launch.clone(),
                tab_index: request.tab_index,
            },
            ArchivedPagePosition {
                leaf_pane_id: request.leaf_pane_id.clone(),
                stack_index: request.stack_index,
                pane_path: request.pane_path.clone(),
            },
        ))
    }
}

fn capture_tab(world: &mut World, tab: Entity) {
    let Some(root) = spawn_tab_root(world, tab) else {
        return;
    };
    let mut state = bevy::ecs::system::SystemState::<TabArchiveLayout>::new(world);
    let selected_stack = state
        .get(world)
        .ok()
        .and_then(|layout| layout.focus.resolve(Some(tab)).2);
    let mut pending = vec![(tab, root)];
    let mut selected_page = None;
    let mut active = None;
    while let Some((live_parent, archive_parent)) = pending.pop() {
        if live_parent != tab && world.get::<PaneSplit>(live_parent).is_none() {
            capture_tab_pages(
                world,
                &mut state,
                root,
                live_parent,
                archive_parent,
                selected_stack,
                &mut selected_page,
                &mut active,
            );
            continue;
        }
        let children = world
            .get::<Children>(live_parent)
            .map(|children| {
                children
                    .iter()
                    .filter(|child| world.get::<Pane>(*child).is_some())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (index, live) in children.into_iter().enumerate().rev() {
            let id = world
                .get::<PaneId>(live)
                .map(|id| id.0.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let weight = world
                .get::<PaneSize>(live)
                .map_or(1.0, |size| size.flex_grow);
            let axis = world
                .get::<PaneSplit>(live)
                .map(|split| match split.direction {
                    PaneSplitDirection::Row => SplitAxis::Row,
                    PaneSplitDirection::Column => SplitAxis::Column,
                });
            let archive = world
                .spawn((
                    topology::Id(id),
                    topology::Weight(weight),
                    vmux_ecs::Order(index as u32),
                    ChildOf(archive_parent),
                ))
                .id();
            if let Some(axis) = axis {
                world.entity_mut(archive).insert(topology::Split::new(axis));
            } else {
                world.entity_mut(archive).insert(topology::Leaf);
            }
            pending.push((live, archive));
        }
    }
    if let Some(page) = selected_page.or_else(|| active.map(|(page, _)| page)) {
        world.entity_mut(page).insert(Active);
    } else {
        world.despawn(root);
    }
    world.flush();
}

fn spawn_tab_root(world: &mut World, tab: Entity) -> Option<Entity> {
    let metadata = world.get::<Tab>(tab)?;
    let name = metadata.name.clone();
    let directory = metadata.startup_dir.clone();
    let space = world.get::<ChildOf>(tab)?.parent();
    let space_id = world.get::<SpaceId>(space)?.0.clone();
    let index = world
        .get::<Children>(space)?
        .iter()
        .filter(|child| world.get::<Tab>(*child).is_some())
        .position(|child| child == tab);
    Some(
        world
            .spawn((
                topology::Root,
                topology::TabGroup,
                topology::Materialized,
                topology::Id(uuid::Uuid::new_v4().to_string()),
                Name::new(name),
                topology::TabDirectory(directory),
                CreatedAt(UnixMillis::now().0),
                topology::Origin {
                    space_id,
                    tab_index: index,
                },
            ))
            .id(),
    )
}

fn capture_tab_pages(
    world: &mut World,
    state: &mut bevy::ecs::system::SystemState<TabArchiveLayout>,
    root: Entity,
    live: Entity,
    archive: Entity,
    selected_stack: Option<Entity>,
    selected_page: &mut Option<Entity>,
    active: &mut Option<(Entity, i64)>,
) {
    let stacks = world
        .get::<Children>(live)
        .map(|children| {
            children
                .iter()
                .filter(|stack| {
                    world.get::<Stack>(*stack).is_some() && world.get::<Retiring>(*stack).is_none()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (index, stack) in stacks.into_iter().enumerate() {
        let request = state.get(world).unwrap().request_page(stack, true);
        let Some(request) = request else {
            continue;
        };
        let closed_at = world.get::<CreatedAt>(root).unwrap().0;
        let Some((intent, metadata, _)) = PageCaptureIntent::new(&request, closed_at, true) else {
            continue;
        };
        let activated = world.get::<LastActivatedAt>(stack).map_or(0, |time| time.0);
        let page = world
            .spawn((
                metadata,
                topology::Materialized,
                vmux_ecs::Order(index as u32),
                ChildOf(archive),
            ))
            .id();
        if selected_stack == Some(stack) {
            *selected_page = Some(page);
        }
        if active.is_none_or(|(_, time)| activated >= time) {
            *active = Some((page, activated));
        }
        if let Some(workspace) = intent.workspace {
            world.entity_mut(page).insert(workspace);
        }
        if let Some(source) = request.source {
            world.trigger(vmux_ecs::page::ArchiveCaptureRequest {
                archive: page,
                source,
            });
            world.flush();
        }
    }
}

fn maintain(roots: Query<(Entity, &CreatedAt), With<topology::Root>>, mut commands: Commands) {
    let now = UnixMillis::now().0;
    let mut retained = Vec::new();
    for (root, closed_at) in &roots {
        if now - closed_at.0 > topology::Root::TTL_MS {
            commands.entity(root).despawn();
        } else {
            retained.push((closed_at.0, root));
        }
    }
    retained.sort_by_key(|(closed_at, root)| (*closed_at, root.to_bits()));
    let excess = retained.len().saturating_sub(topology::Root::CAPACITY);
    for (_, root) in retained.into_iter().take(excess) {
        commands.entity(root).despawn();
    }
}

#[derive(SystemParam)]
pub(super) struct TabArchiveLayout<'w, 's> {
    focus: crate::stack::LayoutFocus<'w, 's>,
    surfaces: crate::hosted_page::StackSurface<'w, 's>,
    surface_metadata: Query<'w, 's, &'static PageMetadata, With<crate::Browser>>,
    stack_pages: Query<
        'w,
        's,
        (
            Entity,
            &'static PageMetadata,
            Option<&'static TerminalLaunch>,
            Option<&'static LastActivatedAt>,
            Option<&'static WorkspaceKind>,
        ),
        (With<Stack>, Without<Retiring>),
    >,
    child_of: Query<'w, 's, &'static ChildOf>,
    children_q: Query<'w, 's, &'static Children>,
    space_hierarchy: SpaceHierarchy<'w, 's>,
    tabs: Query<'w, 's, (), With<Tab>>,
    stacks: Query<'w, 's, (), (With<Stack>, Without<Retiring>)>,
    pane_ids: Query<'w, 's, &'static PaneId>,
    splits: Query<'w, 's, &'static PaneSplit>,
    pane_sizes: Query<'w, 's, &'static PaneSize>,
    panes: Query<'w, 's, (), With<Pane>>,
    window_hierarchy: crate::window::WindowHierarchy<'w, 's>,
}

impl TabArchiveLayout<'_, '_> {
    pub(super) fn capture_stack(&self, stack: Entity, commands: &mut Commands) {
        let Some(request) = self.request(stack) else {
            return;
        };
        if let Some(bundle) = PageCaptureIntent::new(&request, UnixMillis::now().0, false) {
            let operation = commands
                .spawn((
                    vmux_ecs::Operation::bundle("archive.capture-page").unwrap(),
                    vmux_ecs::OperationRetained,
                    bundle,
                ))
                .id();
            commands.trigger(CapturePage { operation });
        }
    }

    fn request(&self, stack: Entity) -> Option<PageArchiveRequest> {
        self.request_page(stack, false)
    }

    fn request_page(&self, stack: Entity, include_empty: bool) -> Option<PageArchiveRequest> {
        let (_, metadata, launch, _, workspace) = self.stack_pages.get(stack).ok()?;
        let source = self.surfaces.current(stack);
        let metadata = source
            .and_then(|source| self.surface_metadata.get(source).ok())
            .unwrap_or(metadata);
        if metadata.url.is_empty() && !include_empty {
            return None;
        }
        let space = self.space_hierarchy.get(stack);
        let (leaf_pane_id, stack_index, pane_path) = self.pane_path(stack).unwrap_or_default();
        Some(PageArchiveRequest {
            source,
            url: metadata.url.clone(),
            title: metadata.title.clone(),
            space_id: self.space_hierarchy.id(stack).unwrap_or_default(),
            launch: launch.cloned(),
            tab_index: space.and_then(|space| self.tab_index(stack, space)),
            leaf_pane_id,
            stack_index,
            pane_path,
            workspace: workspace.copied(),
        })
    }

    fn tab_index(&self, stack: Entity, space: Entity) -> Option<usize> {
        let mut current = stack;
        let tab = loop {
            if self.tabs.contains(current) {
                break current;
            }
            current = self.child_of.get(current).ok()?.parent();
        };
        self.children_q
            .get(space)
            .ok()?
            .iter()
            .filter(|entity| self.tabs.contains(*entity))
            .position(|entity| entity == tab)
    }

    fn pane_path(&self, stack: Entity) -> Option<(String, usize, Vec<PaneStep>)> {
        let leaf = self.child_of.get(stack).ok()?.parent();
        if !self.panes.contains(leaf) {
            return None;
        }
        let leaf_pane_id = self.pane_ids.get(leaf).ok()?.0.clone();
        let stack_index = self
            .children_q
            .get(leaf)
            .ok()?
            .iter()
            .filter(|entity| self.stacks.contains(*entity))
            .position(|entity| entity == stack)?;

        let mut steps = Vec::new();
        let mut current = leaf;
        loop {
            let parent = self.child_of.get(current).ok()?.parent();
            if self.tabs.contains(parent) {
                break;
            }
            let split = self.splits.get(parent).ok()?;
            let pane_children = self
                .children_q
                .get(parent)
                .map(|children| {
                    children
                        .iter()
                        .filter(|entity| self.panes.contains(*entity))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let child_index = pane_children.iter().position(|entity| *entity == current)?;
            let flex_weights = pane_children
                .iter()
                .map(|entity| {
                    self.pane_sizes
                        .get(*entity)
                        .map(|size| size.flex_grow)
                        .unwrap_or(1.0)
                })
                .collect();
            steps.push(PaneStep {
                split_id: self.pane_ids.get(parent).ok()?.0.clone(),
                axis: match split.direction {
                    PaneSplitDirection::Row => SplitAxis::Row,
                    PaneSplitDirection::Column => SplitAxis::Column,
                },
                child_index,
                flex_weights,
            });
            current = parent;
        }
        steps.reverse();
        Some((leaf_pane_id, stack_index, steps))
    }
}

#[derive(SystemParam)]
struct TabCloseLayout<'w, 's> {
    active_tab: ActiveTabParam<'w, 's>,
    hierarchy: TabHierarchy<'w, 's>,
    tabs: Query<'w, 's, &'static Tab>,
    retiring: Query<'w, 's, (), With<Retiring>>,
    transitions: Query<'w, 's, &'static Transition>,
    presentation: Projection<'w, 's>,
    surfaces: Query<'w, 's, (), With<crate::Browser>>,
    archive: TabArchiveLayout<'w, 's>,
    primary_window: Query<'w, 's, Entity, With<PrimaryWindow>>,
    layout_requests: MessageWriter<'w, TabLayoutSpawnRequest>,
    commands: Commands<'w, 's>,
}

impl TabCloseLayout<'_, '_> {
    fn next(&self, tab: Entity, siblings: &[Entity], closing: &HashSet<Entity>) -> Option<Entity> {
        let mut eligible = Vec::new();
        for sibling in siblings {
            if *sibling == tab || !closing.contains(sibling) && !self.retiring.contains(*sibling) {
                eligible.push(*sibling);
            }
        }
        Tab::next_after_close(tab, &eligible)
    }

    fn preferred(&self, tab: Entity, siblings: &[Entity], closing: &HashSet<Entity>) -> Entity {
        self.active_tab
            .get()
            .filter(|active| siblings.contains(active) && closing.contains(active))
            .unwrap_or(tab)
    }

    fn has_surface(&self, entity: Entity) -> bool {
        if self.surfaces.contains(entity) {
            return true;
        }
        self.archive
            .children_q
            .get(entity)
            .is_ok_and(|children| children.iter().any(|child| self.has_surface(child)))
    }
}

fn handle_close_tab_requests(
    mut reader: MessageReader<CloseTabRequest>,
    mut layout: TabCloseLayout,
) {
    let mut seen = HashSet::new();
    let requests: Vec<Entity> = reader
        .read()
        .filter_map(|request| seen.insert(request.tab).then_some(request.tab))
        .filter(|tab| layout.tabs.contains(*tab) && !layout.retiring.contains(*tab))
        .collect();
    let closing: HashSet<Entity> = requests.iter().copied().collect();
    let mut replacement_spaces = HashSet::new();
    for requested_tab in requests {
        let request = CloseTabRequest { tab: requested_tab };
        let Ok(_) = layout.tabs.get(request.tab) else {
            continue;
        };
        if let Some((incoming, mut transition)) = layout.presentation.retaining(request.tab) {
            transition.retire(request.tab);
            layout.commands.spawn((
                vmux_ecs::Operation::bundle("archive.capture-tab").unwrap(),
                vmux_ecs::OperationRetained,
                TabCaptureIntent { tab: request.tab },
            ));
            layout.commands.trigger(TabClosed);
            layout
                .commands
                .entity(request.tab)
                .insert(PendingTabClose { retire: true });
            layout.commands.entity(incoming).insert(transition);
            continue;
        }
        let siblings = layout.hierarchy.siblings(request.tab);
        let surviving_siblings: Vec<Entity> = siblings
            .iter()
            .copied()
            .filter(|sibling| !closing.contains(sibling) && !layout.retiring.contains(*sibling))
            .collect();
        let active = layout.active_tab.get() == Some(request.tab);
        let mut deferred = false;
        if surviving_siblings.is_empty() {
            let Ok(tab_space) = layout
                .archive
                .child_of
                .get(request.tab)
                .map(|parent| parent.parent())
            else {
                continue;
            };
            let preferred_source = layout.preferred(request.tab, &siblings, &closing);
            if request.tab == preferred_source && !replacement_spaces.contains(&tab_space) {
                let Some(window) = layout
                    .archive
                    .window_hierarchy
                    .get(request.tab)
                    .or_else(|| layout.primary_window.single().ok())
                else {
                    continue;
                };
                layout.layout_requests.write(TabLayoutSpawnRequest {
                    space: tab_space,
                    primary_window: window,
                    name: Some("Tab 1".to_string()),
                    startup_dir: None,
                    content: TabLayoutSpawnContent::startup_url_or_prompt(),
                    clear_pending_stack: true,
                    focus: true,
                });
                replacement_spaces.insert(tab_space);
                deferred = layout.has_surface(request.tab);
            }
        } else if active && let Some(next) = layout.next(request.tab, &siblings, &closing) {
            deferred = layout.has_surface(request.tab);
            if deferred {
                let pending = layout.transitions.get(request.tab).ok();
                let visible = layout
                    .presentation
                    .retained(request.tab)
                    .unwrap_or(request.tab);
                layout.commands.entity(next).insert((
                    LastActivatedAt::now(),
                    Transition::retiring(visible, request.tab, pending),
                ));
            } else {
                layout.commands.entity(next).insert(LastActivatedAt::now());
            }
        }

        layout.commands.spawn((
            vmux_ecs::Operation::bundle("archive.capture-tab").unwrap(),
            vmux_ecs::OperationRetained,
            TabCaptureIntent { tab: request.tab },
        ));
        layout.commands.trigger(TabClosed);
        layout
            .commands
            .entity(request.tab)
            .insert(PendingTabClose { retire: deferred });
    }
}

#[derive(SystemParam)]
#[allow(unknown_lints, vmux_ecs_ownership)]
struct ReopenLayout<'w, 's> {
    active_tab: ActiveTabParam<'w, 's>,
    focus: crate::stack::LayoutFocus<'w, 's>,
    surfaces: crate::hosted_page::StackSurface<'w, 's>,
    item_history: Query<'w, 's, &'static vmux_ecs::page::PageItemHistory>,
    window_hierarchy: crate::window::WindowHierarchy<'w, 's>,
    spaces: Query<'w, 's, (Entity, &'static SpaceId), With<Space>>,
    any_space: Query<'w, 's, Entity, With<Space>>,
    current_space: Query<'w, 's, Entity, With<CurrentSpace>>,
    focused_window: crate::window::FocusedWindow<'w, 's>,
    primary_window: Query<'w, 's, Entity, With<PrimaryWindow>>,
}

impl ReopenLayout<'_, '_> {
    fn target(&self, root: Entity, space_id: &str) -> Option<restore::RestoreIntent> {
        let focused_window = self.focused_window.entity();
        let origin_space = self
            .spaces
            .iter()
            .filter(|(_, id)| id.0 == space_id)
            .find(|(space, _)| {
                focused_window
                    .is_some_and(|focused| self.window_hierarchy.get(*space) == Some(focused))
            })
            .or_else(|| self.spaces.iter().find(|(_, id)| id.0 == space_id))
            .map(|(entity, _)| entity);
        let space = origin_space
            .or_else(|| {
                self.current_space
                    .iter()
                    .find(|entity| self.any_space.contains(*entity))
            })
            .or_else(|| self.any_space.iter().next())?;
        let bound_window = self.window_hierarchy.get(space);
        let window = bound_window
            .or(focused_window)
            .or_else(|| self.primary_window.single().ok())?;
        Some(restore::RestoreIntent {
            root,
            space,
            origin_matches: origin_space == Some(space),
            window,
            window_bound: bound_window.is_some(),
        })
    }
}

#[derive(SystemParam)]
#[allow(unknown_lints, vmux_ecs_ownership)]
struct ReopenRequest<'w, 's> {
    reader: MessageReader<'w, 's, ReopenClosedPage>,
    archived: Query<
        'w,
        's,
        (Entity, &'static CreatedAt, &'static topology::Origin),
        (
            With<topology::Root>,
            Without<restore::Claimed>,
            Without<topology::InvalidTopology>,
        ),
    >,
    layout: ReopenLayout<'w, 's>,
}

fn handle_reopen_closed_page(
    world: &mut World,
    state: &mut bevy::ecs::system::SystemState<ReopenRequest>,
) {
    let (candidate, latest, item_page) = {
        let Ok(mut params) = state.get_mut(world) else {
            return;
        };
        if params.reader.read().count() == 0 {
            return;
        }
        let candidate = params
            .archived
            .iter()
            .max_by_key(|(root, closed_at, _)| (closed_at.0, root.to_bits()))
            .map(|(root, closed_at, origin)| (root, closed_at.0, origin.space_id.clone()));
        let latest = candidate.as_ref().map_or(0, |(_, closed_at, _)| *closed_at);
        let (_, _, stack) = params.layout.focus.resolve(params.layout.active_tab.get());
        let item_page = stack
            .and_then(|stack| params.layout.surfaces.current(stack))
            .filter(|page| {
                params
                    .layout
                    .item_history
                    .get(*page)
                    .is_ok_and(|history| history.closed_at >= latest)
            });
        (candidate, latest, item_page)
    };
    if let Some(page) = item_page {
        let request = vmux_ecs::page::ReopenItemRequest::new(page, latest);
        world.trigger(request.clone());
        world.flush();
        if request.handled() {
            return;
        }
    }
    let Some((root, _, space_id)) = candidate else {
        return;
    };
    let target = state
        .get(world)
        .ok()
        .and_then(|params| params.layout.target(root, &space_id));
    let Some(target) = target else {
        return;
    };
    let operation = world
        .spawn((
            Name::new("Restore archived layout"),
            vmux_ecs::Operation::bundle("archive.restore").unwrap(),
            vmux_ecs::OperationRetained,
        ))
        .id();
    world.trigger(restore::RestoreRequest {
        operation,
        root,
        space: target.space,
        window: target.window,
        origin_matches: target.origin_matches,
        window_bound: target.window_bound,
    });
    world.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::relationship::Relationship;
    use bevy::ecs::system::Command;

    use crate::pane::{Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection};

    struct Fixture {
        app: App,
    }

    impl std::ops::Deref for Fixture {
        type Target = App;

        fn deref(&self) -> &App {
            &self.app
        }
    }

    impl std::ops::DerefMut for Fixture {
        fn deref_mut(&mut self) -> &mut App {
            &mut self.app
        }
    }

    impl Fixture {
        fn page(url: &str, closed_at: i64) -> ArchivedPage {
            ArchivedPage {
                url: url.to_string(),
                title: String::new(),
                space_id: "s".to_string(),
                closed_at,
                launch: None,
                tab_index: None,
            }
        }

        fn drain_archive_reqs(&mut self) -> Vec<PageArchiveRequest> {
            self.app
                .world_mut()
                .resource_mut::<Messages<PageArchiveRequest>>()
                .drain()
                .collect()
        }

        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(ArchivePlugin)
                .add_message::<CloseStackRequest>()
                .init_resource::<LayoutSettings>();
            app.world_mut()
                .spawn((bevy::window::Window::default(), PrimaryWindow));
            Self { app }
        }

        fn request_close(&mut self, stack: Entity) {
            self.app
                .world_mut()
                .resource_mut::<Messages<CloseStackRequest>>()
                .write(CloseStackRequest::by_user(stack));
            self.app.update();
        }

        fn closing() -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                crate::stack::StackPlugin,
                crate::presentation::PresentationPlugin,
                ArchivePlugin,
            ))
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<LayoutSettings>();
            app.world_mut()
                .spawn((bevy::window::Window::default(), PrimaryWindow));
            Self { app }
        }

        fn spawn_inactive_and_active_stacks(&mut self) -> (Entity, Entity) {
            let space = self
                .app
                .world_mut()
                .spawn((Space, SpaceId("s1".to_string())))
                .id();
            let tab = self
                .app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(1), ChildOf(space)))
                .id();
            let pane = self
                .app
                .world_mut()
                .spawn((
                    Pane,
                    PaneId("leaf".to_string()),
                    LastActivatedAt(1),
                    ChildOf(tab),
                ))
                .id();
            let inactive = self
                .app
                .world_mut()
                .spawn((
                    Stack::default(),
                    PageMetadata {
                        url: "https://inactive.example".to_string(),
                        ..default()
                    },
                    LastActivatedAt(1),
                    ChildOf(pane),
                ))
                .id();
            let active = self
                .app
                .world_mut()
                .spawn((
                    Stack::default(),
                    PageMetadata {
                        url: "https://active.example".to_string(),
                        ..default()
                    },
                    LastActivatedAt(2),
                    ChildOf(pane),
                ))
                .id();
            (inactive, active)
        }

        fn archived_urls(&mut self) -> Vec<String> {
            self.app
                .world_mut()
                .query::<&ArchivedPage>()
                .iter(self.app.world())
                .map(|page| page.url.clone())
                .collect()
        }

        fn dispatch_reopen(&mut self) {
            self.app
                .world_mut()
                .resource_mut::<Messages<ReopenClosedPage>>()
                .write(ReopenClosedPage);
            self.app.update();
        }

        fn drain_opens(&mut self) -> Vec<PageOpenRequest> {
            self.app
                .world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .collect()
        }
    }

    #[test]
    fn captured_canonical_tab_survives_store_reload_with_empty_leaves_weights_and_selection() {
        let mut app = Fixture::new();
        app.add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_type::<Name>();
        let space = app.world_mut().spawn((Space, SpaceId("s1".into()))).id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneId("root".into()),
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        app.world_mut().spawn((
            Pane,
            PaneId("empty".into()),
            PaneSize { flex_grow: 3.0 },
            ChildOf(root),
        ));
        let nested = app
            .world_mut()
            .spawn((
                Pane,
                PaneId("nested".into()),
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                PaneSize { flex_grow: 2.0 },
                ChildOf(root),
            ))
            .id();
        for (id, url, weight) in [
            ("first", "https://first.example", 2.0),
            ("second", "https://second.example", 1.0),
        ] {
            let leaf = app
                .world_mut()
                .spawn((
                    Pane,
                    PaneId(id.into()),
                    PaneSize { flex_grow: weight },
                    ChildOf(nested),
                ))
                .id();
            app.world_mut().spawn((
                Stack::default(),
                PageMetadata {
                    url: url.into(),
                    ..Default::default()
                },
                LastActivatedAt(1),
                ChildOf(leaf),
            ));
        }
        app.world_mut().write_message(CloseTabRequest { tab });
        app.update();
        let archive_root = app
            .world_mut()
            .query_filtered::<Entity, With<topology::TabGroup>>()
            .single(app.world())
            .unwrap();
        let mut entities = Vec::new();
        let mut pending = vec![archive_root];
        while let Some(entity) = pending.pop() {
            entities.push(entity);
            if let Some(children) = app.world().get::<Children>(entity) {
                pending.extend(children.iter());
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("archive.ron");
        let mut save = moonshine_save::save::SaveWorld::default_into_file(&path);
        save.entities = moonshine_save::save::EntityFilter::allow(entities);
        moonshine_save::save::SaveCommand(save)
            .apply(app.world_mut())
            .unwrap();
        let mut restored = Fixture::new();
        restored.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        restored
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_type::<Name>();
        moonshine_save::load::LoadCommand(moonshine_save::load::LoadWorld::default_from_file(
            &path,
        ))
        .apply(restored.world_mut())
        .unwrap();
        restored.world_mut().spawn((Space, SpaceId("s1".into())));
        restored.dispatch_reopen();
        let panes = restored
            .world_mut()
            .query::<(Entity, &PaneId, &PaneSize, &vmux_flex::Node)>()
            .iter(restored.world())
            .map(|(entity, id, size, node)| (entity, id.0.clone(), size.flex_grow, node.flex_grow))
            .collect::<Vec<_>>();
        assert_eq!(panes.len(), 5);
        let empty = panes.iter().find(|(_, id, _, _)| id == "empty").unwrap();
        assert_eq!(empty.2, 3.0);
        assert_eq!(empty.3, 3.0);
        assert!(
            restored
                .world()
                .get::<Children>(empty.0)
                .is_none_or(|children| children.is_empty())
        );
        assert!(
            panes
                .iter()
                .any(|(_, id, weight, node)| id == "nested" && *weight == 2.0 && *node == 2.0)
        );
        let selected = restored
            .world_mut()
            .query::<(&PageMetadata, &LastActivatedAt)>()
            .iter(restored.world())
            .max_by_key(|(_, time)| time.0)
            .unwrap()
            .0
            .url
            .clone();
        assert_eq!(selected, "https://second.example");
        let urls = restored
            .drain_opens()
            .into_iter()
            .map(|open| open.url)
            .collect::<HashSet<_>>();
        assert_eq!(
            urls,
            HashSet::from([
                "https://first.example".into(),
                "https://second.example".into()
            ])
        );
        assert_eq!(
            restored
                .world_mut()
                .query::<&ArchivedPage>()
                .iter(restored.world())
                .count(),
            0
        );
    }

    #[test]
    fn conflicting_legacy_group_retains_both_pages_and_refuses_live_allocation() {
        let mut app = Fixture::new();
        app.world_mut().spawn((Space, SpaceId("s1".into())));
        let closed_at = UnixMillis::now().0;
        for (index, weight) in [(0, 2.0), (1, 3.0)] {
            app.world_mut().spawn((
                ArchivedPage {
                    url: format!("https://retained-{index}.example"),
                    space_id: "s1".into(),
                    closed_at,
                    ..Default::default()
                },
                ArchivedTabPage {
                    group_id: "conflict".into(),
                    tab_name: "Retained".into(),
                    tab_startup_dir: None,
                    active: index == 1,
                },
                ArchivedPagePosition {
                    leaf_pane_id: format!("leaf-{index}"),
                    stack_index: 0,
                    pane_path: vec![PaneStep {
                        split_id: "root".into(),
                        axis: SplitAxis::Row,
                        child_index: index,
                        flex_weights: vec![1.0, weight],
                    }],
                },
            ));
        }
        app.dispatch_reopen();
        assert!(app.drain_opens().is_empty());
        assert_eq!(
            app.world_mut()
                .query::<&ArchivedPage>()
                .iter(app.world())
                .count(),
            2
        );
        assert_eq!(
            app.world_mut().query::<&Stack>().iter(app.world()).count(),
            0
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, (With<topology::Root>, With<topology::InvalidTopology>)>()
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn reopen_burst_is_coalesced_and_fully_consumed_in_the_input_frame() {
        let mut app = Fixture::new();
        app.world_mut().spawn((Space, SpaceId("s1".into())));
        let now = UnixMillis::now().0;
        for (url, closed_at) in [
            ("https://older.example", now),
            ("https://newer.example", now + 1),
        ] {
            app.world_mut().spawn(ArchivedPage {
                url: url.into(),
                space_id: "s1".into(),
                closed_at,
                ..Default::default()
            });
        }
        for _ in 0..3 {
            app.world_mut().write_message(ReopenClosedPage);
        }
        app.update();
        let opens = app.drain_opens();
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://newer.example");
        app.update();
        assert!(app.drain_opens().is_empty());
        assert_eq!(
            app.world_mut()
                .query::<&ArchivedPage>()
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn reopening_an_old_leaf_under_a_retiring_tab_uses_a_live_new_tab() {
        let mut app = Fixture::new();
        let space = app.world_mut().spawn((Space, SpaceId("s1".into()))).id();
        let retiring = app
            .world_mut()
            .spawn((Tab::default(), Retiring, ChildOf(space)))
            .id();
        let leaf = app
            .world_mut()
            .spawn((Pane, PaneId("old".into()), ChildOf(retiring)))
            .id();
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://retained.example".into(),
                space_id: "s1".into(),
                closed_at: UnixMillis::now().0,
                ..Default::default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "old".into(),
                ..Default::default()
            },
        ));
        app.dispatch_reopen();
        let opens = app.drain_opens();
        let PageOpenTarget::Stack(stack) = opens[0].target else {
            panic!("explicit restored stack");
        };
        let live_leaf = app.world().get::<ChildOf>(stack).unwrap().parent();
        assert_ne!(live_leaf, leaf);
        let live_tab = app.world().get::<ChildOf>(live_leaf).unwrap().parent();
        assert_ne!(live_tab, retiring);
        assert!(app.world().get::<Retiring>(live_tab).is_none());
    }

    #[test]
    fn capture_spawns_archived_page() {
        let mut app = Fixture::new();
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                source: None,
                url: "https://a.example".to_string(),
                title: "A".to_string(),
                space_id: "s".to_string(),
                launch: None,
                tab_index: None,
                leaf_pane_id: String::new(),
                stack_index: 0,
                pane_path: Vec::new(),
                workspace: Some(WorkspaceKind::Knowledge),
            });
        app.update();
        let mut q = app.world_mut().query::<(&ArchivedPage, &WorkspaceKind)>();
        let all: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0.url, "https://a.example");
        assert_eq!(*all[0].1, WorkspaceKind::Knowledge);
    }

    #[test]
    fn capture_skips_empty_url() {
        let mut app = Fixture::new();
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                source: None,
                url: String::new(),
                title: String::new(),
                space_id: "s".to_string(),
                launch: None,
                tab_index: None,
                leaf_pane_id: String::new(),
                stack_index: 0,
                pane_path: Vec::new(),
                workspace: None,
            });
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }

    #[test]
    fn capture_materializes_a_weighted_path_before_reopen() {
        let mut app = Fixture::new();
        app.world_mut().write_message(PageArchiveRequest {
            source: None,
            url: "https://a.example".into(),
            title: "A".into(),
            space_id: "s".into(),
            launch: None,
            tab_index: Some(0),
            leaf_pane_id: "leaf-1".into(),
            stack_index: 2,
            pane_path: vec![PaneStep {
                split_id: "root".into(),
                axis: SplitAxis::Row,
                child_index: 1,
                flex_weights: vec![1.0, 2.0],
            }],
            workspace: None,
        });
        app.update();
        let (page, parent, order) = app
            .world_mut()
            .query::<(&ArchivedPage, &ChildOf, &vmux_ecs::Order)>()
            .single(app.world())
            .unwrap();
        assert_eq!(page.url, "https://a.example");
        assert_eq!(order.0, 2);
        let leaf = parent.parent();
        assert_eq!(app.world().get::<topology::Id>(leaf).unwrap().0, "leaf-1");
        assert_eq!(app.world().get::<topology::Weight>(leaf).unwrap().0, 2.0);
        let split = app.world().get::<ChildOf>(leaf).unwrap().parent();
        assert_eq!(app.world().get::<topology::Id>(split).unwrap().0, "root");
        assert_eq!(app.world().get::<Children>(split).unwrap().len(), 2);
        assert_eq!(
            app.world_mut()
                .query::<&ArchivedPagePosition>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn maintain_enforces_cap_dropping_oldest() {
        let mut app = Fixture::new();
        let now = UnixMillis::now().0;
        for i in 0..(topology::Root::CAPACITY as i64 + 1) {
            app.world_mut()
                .spawn(Fixture::page(&format!("u{i}"), now - i));
        }
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls.len(), topology::Root::CAPACITY);
        let oldest = format!("u{}", topology::Root::CAPACITY);
        assert!(!urls.contains(&oldest));
    }

    #[test]
    fn maintain_purges_expired() {
        let mut app = Fixture::new();
        let now = UnixMillis::now().0;
        app.world_mut().spawn(Fixture::page("fresh", now));
        app.world_mut()
            .spawn(Fixture::page("stale", now - topology::Root::TTL_MS - 1));
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls, vec!["fresh".to_string()]);
    }

    #[test]
    fn close_request_archives_the_named_stack() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://gone.example".to_string(),
                    title: "Gone".to_string(),
                    ..default()
                },
                ChildOf(space),
            ))
            .id();
        app.request_close(stack);
        let reqs = app.drain_archive_reqs();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "https://gone.example");
        assert_eq!(reqs[0].space_id, "s1");
    }

    #[test]
    fn duplicate_close_requests_archive_the_stack_once() {
        let mut app = Fixture::new();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://gone.example".to_string(),
                    ..default()
                },
            ))
            .id();
        for _ in 0..3 {
            app.world_mut()
                .resource_mut::<Messages<CloseStackRequest>>()
                .write(CloseStackRequest::by_user(stack));
        }

        app.update();

        assert_eq!(app.drain_archive_reqs().len(), 1);
    }

    #[test]
    fn tidying_a_stack_never_reaches_the_reopen_list() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://tidied.example".to_string(),
                    ..default()
                },
                ChildOf(space),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::tidying(stack));
        app.update();
        assert!(app.drain_archive_reqs().is_empty());
    }

    #[test]
    fn close_request_skips_empty_url_stack() {
        let mut app = Fixture::new();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), PageMetadata::default()))
            .id();
        app.request_close(stack);
        assert!(app.drain_archive_reqs().is_empty());
    }

    #[test]
    fn close_records_tab_index_of_closing_stack() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        app.world_mut().spawn((Tab::default(), ChildOf(space)));
        let tab1 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let pane = app.world_mut().spawn(ChildOf(tab1)).id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://gone.example".to_string(),
                    ..default()
                },
                ChildOf(pane),
            ))
            .id();
        app.request_close(stack);
        let reqs = app.drain_archive_reqs();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].tab_index, Some(1));
    }

    #[test]
    fn close_records_pane_path_and_leaf() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        let leaf0 = app
            .world_mut()
            .spawn((
                Pane,
                PaneId("leaf0".to_string()),
                PaneSize { flex_grow: 1.0 },
                ChildOf(root),
            ))
            .id();
        let leaf1 = app
            .world_mut()
            .spawn((
                Pane,
                PaneId("leaf1".to_string()),
                PaneSize { flex_grow: 3.0 },
                ChildOf(root),
            ))
            .id();
        let _ = leaf0;
        app.world_mut().spawn((Stack::default(), ChildOf(leaf1)));
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://z.example".to_string(),
                    ..default()
                },
                ChildOf(leaf1),
            ))
            .id();
        app.request_close(stack);
        let reqs = app.drain_archive_reqs();
        assert_eq!(reqs.len(), 1);
        let req = &reqs[0];
        assert_eq!(req.leaf_pane_id, "leaf1");
        assert_eq!(req.stack_index, 1);
        assert_eq!(req.pane_path.len(), 1);
        assert_eq!(req.pane_path[0].split_id, "root");
        assert_eq!(req.pane_path[0].child_index, 1);
        assert_eq!(req.pane_path[0].flex_weights, vec![1.0, 3.0]);
        assert!(matches!(req.pane_path[0].axis, SplitAxis::Row));
    }

    #[test]
    fn closing_an_inactive_stack_archives_it_and_not_the_active_one() {
        let mut app = Fixture::closing();
        let (inactive, active) = app.spawn_inactive_and_active_stacks();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest::by_user(inactive));
        app.update();
        app.update();

        assert!(app.world().get_entity(inactive).is_err());
        assert!(app.world().get_entity(active).is_ok());
        assert_eq!(
            app.archived_urls(),
            vec!["https://inactive.example".to_string()]
        );
    }

    #[test]
    fn close_command_archives_the_active_stack_before_the_successor_retires_it() {
        let mut app = Fixture::closing();
        let (inactive, active) = app.spawn_inactive_and_active_stacks();

        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);
        app.update();
        app.update();

        assert!(app.world().get_entity(active).is_ok());
        assert_eq!(
            app.archived_urls(),
            vec!["https://active.example".to_string()]
        );

        app.world_mut().spawn((
            crate::cef::Browser,
            vmux_ecs::page::PageReady {},
            vmux_ecs::PagePresented,
            crate::hosted_page::SurfacePresentationReady,
            ChildOf(inactive),
        ));
        for _ in 0..3 {
            app.update();
        }

        assert!(app.world().get_entity(active).is_err());
    }

    #[test]
    fn close_tab_archives_every_stack_in_one_group() {
        let mut app = App::new();
        app.add_plugins(ArchivePlugin)
            .add_message::<TabLayoutSpawnRequest>();
        app.world_mut()
            .spawn((bevy::window::Window::default(), PrimaryWindow));
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string()), Active))
            .id();
        app.world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(space)));
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Work".to_string(),
                    startup_dir: Some("/tmp/work".to_string()),
                },
                LastActivatedAt(2),
                Active,
                ChildOf(space),
            ))
            .id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        let left = app
            .world_mut()
            .spawn((
                Pane,
                PaneId("left".to_string()),
                PaneSize { flex_grow: 1.0 },
                ChildOf(root),
            ))
            .id();
        let right = app
            .world_mut()
            .spawn((
                Pane,
                PaneId("right".to_string()),
                PaneSize { flex_grow: 2.0 },
                ChildOf(root),
            ))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                url: "https://left.example".to_string(),
                ..default()
            },
            LastActivatedAt(3),
            ChildOf(left),
        ));
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                url: "https://right.example".to_string(),
                ..default()
            },
            LastActivatedAt(4),
            ChildOf(right),
        ));
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata::default(),
            LastActivatedAt(2),
            ChildOf(left),
        ));
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                url: "https://retiring.example".to_string(),
                ..default()
            },
            LastActivatedAt(5),
            Retiring,
            ChildOf(right),
        ));
        app.world_mut()
            .resource_mut::<Messages<CloseTabRequest>>()
            .write(CloseTabRequest { tab });

        app.update();

        assert!(app.world().get_entity(tab).is_err());
        let roots = app
            .world_mut()
            .query_filtered::<Entity, With<topology::TabGroup>>()
            .iter(app.world())
            .collect::<Vec<_>>();
        assert_eq!(roots.len(), 1);
        let archive_root = roots[0];
        assert_eq!(
            app.world().get::<Name>(archive_root).unwrap().as_str(),
            "Work"
        );
        assert_eq!(
            app.world()
                .get::<topology::TabDirectory>(archive_root)
                .unwrap()
                .0
                .as_deref(),
            Some("/tmp/work")
        );
        let archived = app
            .world_mut()
            .query::<(&ArchivedPage, &ChildOf, Has<Active>)>()
            .iter(app.world())
            .map(|(page, parent, active)| (page.url.clone(), parent.parent(), active))
            .collect::<Vec<_>>();
        assert_eq!(archived.len(), 3);
        assert!(
            archived
                .iter()
                .all(|entry| entry.0 != "https://retiring.example")
        );
        assert_eq!(archived.iter().filter(|entry| entry.2).count(), 1);
        assert!(
            archived
                .iter()
                .any(|entry| entry.0 == "https://right.example" && entry.2)
        );
        let empty = archived.iter().find(|entry| entry.0.is_empty()).unwrap().1;
        assert_eq!(app.world().get::<topology::Id>(empty).unwrap().0, "left");
        assert_eq!(
            app.world_mut()
                .query::<&ArchivedPagePosition>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn closing_last_two_tabs_same_frame_requests_one_replacement() {
        let mut app = App::new();
        app.add_plugins(ArchivePlugin)
            .add_message::<TabLayoutSpawnRequest>();
        app.world_mut()
            .spawn((bevy::window::Window::default(), PrimaryWindow));
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string()), Active))
            .id();
        app.world_mut()
            .entity_mut(space)
            .insert(crate::space::EffectiveStartupDir(Some(PathBuf::from(
                "/tmp",
            ))));
        let first = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(2), Active, ChildOf(space)))
            .id();
        let second = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(space)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<CloseTabRequest>>()
            .write(CloseTabRequest { tab: first });
        app.world_mut()
            .resource_mut::<Messages<CloseTabRequest>>()
            .write(CloseTabRequest { tab: second });

        app.update();

        assert!(app.world().get_entity(first).is_err());
        assert!(app.world().get_entity(second).is_err());
        let requests: Vec<TabLayoutSpawnRequest> = app
            .world_mut()
            .resource_mut::<Messages<TabLayoutSpawnRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].space, space);
        assert_eq!(requests[0].startup_dir, None);
        assert!(requests[0].focus);
    }

    #[test]
    fn reopen_targets_materialized_stack() {
        let mut app = Fixture::new();
        app.world_mut().spawn((Space, SpaceId("s1".to_string())));
        app.world_mut().spawn((
            ArchivedPage {
                url: "vmux://terminal/".to_string(),
                space_id: "s1".to_string(),
                closed_at: UnixMillis::now().0 + 5,
                ..default()
            },
            WorkspaceKind::Knowledge,
        ));

        app.dispatch_reopen();
        let opens = app.drain_opens();
        assert_eq!(opens.len(), 1);
        let PageOpenTarget::Stack(stack) = opens[0].target else {
            panic!("reopened page must target a stack");
        };
        assert!(app.world().get::<Stack>(stack).is_some());
        assert!(app.world().get::<PageRestore>(stack).is_some());
        assert_eq!(
            app.world().get::<WorkspaceKind>(stack),
            Some(&WorkspaceKind::Knowledge)
        );
    }

    #[test]
    fn reopen_tab_group_restores_panes_and_stacks_together() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        app.world_mut().spawn((
            Tab {
                name: "Existing".to_string(),
                startup_dir: None,
            },
            ChildOf(space),
        ));
        let closed_at = UnixMillis::now().0 + 10;
        for (url, leaf, stack_index, child_index, active) in [
            ("https://left-1.example", "left", 0, 0, false),
            ("https://left-2.example", "left", 1, 0, false),
            ("", "left", 2, 0, false),
            ("https://right.example", "right", 0, 1, true),
        ] {
            app.world_mut().spawn((
                ArchivedPage {
                    url: url.to_string(),
                    space_id: "s1".to_string(),
                    closed_at,
                    tab_index: Some(1),
                    ..default()
                },
                ArchivedPagePosition {
                    leaf_pane_id: leaf.to_string(),
                    stack_index,
                    pane_path: vec![PaneStep {
                        split_id: "root".to_string(),
                        axis: SplitAxis::Row,
                        child_index,
                        flex_weights: vec![1.0, 2.0],
                    }],
                },
                ArchivedTabPage {
                    group_id: "group-1".to_string(),
                    tab_name: "Recovered".to_string(),
                    tab_startup_dir: Some("/tmp/recovered".to_string()),
                    active,
                },
            ));
        }

        app.dispatch_reopen();

        let tabs: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<Tab>>()
            .iter(app.world())
            .collect();
        assert_eq!(tabs.len(), 2);
        let recovered = tabs
            .into_iter()
            .find(|entity| {
                app.world()
                    .get::<Tab>(*entity)
                    .is_some_and(|tab| tab.name == "Recovered")
            })
            .expect("recovered tab");
        let recovered_tab = app.world().get::<Tab>(recovered).unwrap();
        assert_eq!(recovered_tab.startup_dir.as_deref(), Some("/tmp/recovered"));
        let space_tabs: Vec<Entity> = app
            .world()
            .get::<Children>(space)
            .unwrap()
            .iter()
            .filter(|entity| app.world().get::<Tab>(*entity).is_some())
            .collect();
        assert_eq!(space_tabs[1], recovered);
        let root = app
            .world_mut()
            .query::<(Entity, &PaneId)>()
            .iter(app.world())
            .find(|(_, id)| id.0 == "root")
            .map(|(entity, _)| entity)
            .expect("root pane");
        assert_eq!(
            app.world()
                .get::<ChildOf>(root)
                .map(|parent| parent.parent()),
            Some(recovered)
        );
        assert_eq!(
            app.world()
                .get::<PaneSplit>(root)
                .map(|split| split.direction),
            Some(PaneSplitDirection::Row)
        );
        assert!(
            app.world()
                .get::<LastActivatedAt>(root)
                .is_some_and(|activated| activated.0 > 0)
        );
        for (leaf_id, expected_urls) in [
            (
                "left",
                vec![
                    "https://left-1.example".to_string(),
                    "https://left-2.example".to_string(),
                    String::new(),
                ],
            ),
            ("right", vec!["https://right.example".to_string()]),
        ] {
            let leaf = app
                .world_mut()
                .query::<(Entity, &PaneId)>()
                .iter(app.world())
                .find(|(_, id)| id.0 == leaf_id)
                .map(|(entity, _)| entity)
                .expect("leaf pane");
            let expected_flex = if leaf_id == "left" { 1.0 } else { 2.0 };
            assert_eq!(
                app.world().get::<PaneSize>(leaf).map(|size| size.flex_grow),
                Some(expected_flex)
            );
            let urls: Vec<String> = app
                .world()
                .get::<Children>(leaf)
                .unwrap()
                .iter()
                .filter_map(|stack| app.world().get::<PageMetadata>(stack))
                .map(|metadata| metadata.url.clone())
                .collect();
            assert_eq!(urls, expected_urls);
            if leaf_id == "right" {
                assert!(
                    app.world()
                        .get::<LastActivatedAt>(leaf)
                        .is_some_and(|activated| activated.0 > 0)
                );
                let active_stack = app
                    .world()
                    .get::<Children>(leaf)
                    .unwrap()
                    .iter()
                    .find(|stack| {
                        app.world()
                            .get::<PageMetadata>(*stack)
                            .is_some_and(|metadata| metadata.url == "https://right.example")
                    })
                    .expect("active stack");
                assert!(
                    app.world()
                        .get::<LastActivatedAt>(active_stack)
                        .is_some_and(|activated| activated.0 > 0)
                );
            }
        }
        assert_eq!(
            app.world_mut()
                .query::<(&Stack, &LastActivatedAt)>()
                .iter(app.world())
                .filter(|(_, activated)| activated.0 > 0)
                .count(),
            1
        );
        assert_eq!(app.drain_opens().len(), 3);
        assert_eq!(
            app.world_mut()
                .query::<&ArchivedPage>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn reopen_web_opens_in_origin_space_and_consumes_entry() {
        let mut app = Fixture::new();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "https://a.example".to_string(),
            title: "A".to_string(),
            space_id: "s1".to_string(),
            closed_at: UnixMillis::now().0 + 5,
            launch: None,
            tab_index: None,
        });
        app.dispatch_reopen();

        let opens = app.drain_opens();
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://a.example");
        assert!(matches!(opens[0].target, PageOpenTarget::Stack(_)));
        let mut q = app.world_mut().query::<&ArchivedPage>();
        assert_eq!(q.iter(app.world()).count(), 0);
        let mut metas = app
            .world_mut()
            .query::<(&crate::stack::Stack, &PageMetadata)>();
        assert!(
            metas
                .iter(app.world())
                .any(|(_, m)| m.url == "https://a.example")
        );
    }

    #[test]
    fn reopen_prefers_the_focused_windows_view_of_the_origin_space() {
        let mut app = Fixture::new();
        let primary_window = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .unwrap();
        let secondary_window = app.world_mut().spawn(Window::default()).id();
        app.world_mut().entity_mut(secondary_window).insert(Active);
        let primary_root = app.world_mut().spawn(HostWindow(primary_window)).id();
        let secondary_root = app.world_mut().spawn(HostWindow(secondary_window)).id();
        let primary_space = app
            .world_mut()
            .spawn((Space, SpaceId("shared".to_string()), ChildOf(primary_root)))
            .id();
        let secondary_space = app
            .world_mut()
            .spawn((
                Space,
                SpaceId("shared".to_string()),
                ChildOf(secondary_root),
            ))
            .id();
        app.world_mut().spawn(ArchivedPage {
            url: "https://focused.example".to_string(),
            space_id: "shared".to_string(),
            closed_at: UnixMillis::now().0 + 5,
            ..default()
        });

        app.dispatch_reopen();

        let tab_parents: Vec<Entity> = app
            .world_mut()
            .query_filtered::<&ChildOf, With<Tab>>()
            .iter(app.world())
            .map(ChildOf::parent)
            .collect();
        assert!(tab_parents.contains(&secondary_space));
        assert!(!tab_parents.contains(&primary_space));
    }

    #[test]
    fn reopen_picks_newest_first() {
        let mut app = Fixture::new();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "https://old.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: UnixMillis::now().0 + 1,
            launch: None,
            tab_index: None,
        });
        app.world_mut().spawn(ArchivedPage {
            url: "https://new.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: UnixMillis::now().0 + 2,
            launch: None,
            tab_index: None,
        });
        app.dispatch_reopen();
        let opens = app.drain_opens();
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://new.example");
    }

    #[test]
    fn reopen_terminal_respawns_at_cwd() {
        let mut app = Fixture::new();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "vmux://terminal/".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: UnixMillis::now().0 + 5,
            launch: Some(TerminalLaunch {
                command: "/bin/zsh".to_string(),
                args: vec![],
                cwd: "/work".to_string(),
                env: vec![],
            }),
            tab_index: None,
        });
        app.dispatch_reopen();
        let opens = app.drain_opens();
        assert_eq!(opens.len(), 1);
        let PageOpenTarget::Stack(stack) = opens[0].target else {
            panic!("reopened page must target a stack");
        };
        assert_eq!(
            app.world().get::<TerminalLaunch>(stack).unwrap().cwd,
            "/work"
        );
        assert!(app.world().get::<PageRestore>(stack).is_some());
    }

    #[test]
    fn reopen_empty_archive_is_noop() {
        let mut app = Fixture::new();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.dispatch_reopen();
        assert!(app.drain_opens().is_empty());
    }

    #[test]
    fn reopen_falls_back_to_active_space_when_origin_gone() {
        let mut app = Fixture::new();
        let active = app
            .world_mut()
            .spawn((
                crate::space::Space,
                crate::space::SpaceId("active".to_string()),
            ))
            .id();
        app.world_mut()
            .entity_mut(active)
            .insert(crate::space::CurrentSpace);
        app.world_mut().spawn(ArchivedPage {
            url: "https://x.example".to_string(),
            title: String::new(),
            space_id: "ghost".to_string(),
            closed_at: UnixMillis::now().0 + 5,
            launch: None,
            tab_index: None,
        });
        app.dispatch_reopen();
        let opens = app.drain_opens();
        assert_eq!(opens.len(), 1);
        let mut tabs = app.world_mut().query::<(&crate::tab::Tab, &ChildOf)>();
        assert!(tabs.iter(app.world()).any(|(_, co)| co.get() == active));
    }

    #[test]
    fn reopen_restores_tab_at_original_index() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let t0 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let t1 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        app.world_mut().spawn(ArchivedPage {
            url: "https://z.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: UnixMillis::now().0 + 5,
            launch: None,
            tab_index: Some(0),
        });
        app.dispatch_reopen();

        let tabs_q = app.world().entity(space).get::<Children>().unwrap();
        let tab_order: Vec<Entity> = tabs_q.iter().collect();
        assert_eq!(tab_order.len(), 3);
        assert_ne!(tab_order[0], t0);
        assert_ne!(tab_order[0], t1);
        assert_eq!(tab_order[1], t0);
        assert_eq!(tab_order[2], t1);
    }

    #[test]
    fn reopen_appends_when_origin_space_gone() {
        let mut app = Fixture::new();
        let active = app
            .world_mut()
            .spawn((Space, SpaceId("active".to_string())))
            .id();
        app.world_mut()
            .entity_mut(active)
            .insert(crate::space::CurrentSpace);
        let t0 = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(active)))
            .id();
        let t1 = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(active)))
            .id();
        app.world_mut().spawn(ArchivedPage {
            url: "https://z.example".to_string(),
            title: String::new(),
            space_id: "ghost".to_string(),
            closed_at: UnixMillis::now().0 + 5,
            launch: None,
            tab_index: Some(0),
        });
        app.dispatch_reopen();

        let tabs_q = app.world().entity(active).get::<Children>().unwrap();
        let tab_order: Vec<Entity> = tabs_q.iter().collect();
        assert_eq!(tab_order.len(), 3);
        assert_eq!(tab_order[0], t0);
        assert_eq!(tab_order[1], t1);
        assert_ne!(tab_order[2], t0);
        assert_ne!(tab_order[2], t1);
    }

    #[test]
    fn reopen_into_surviving_leaf_pane_at_index() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let leaf = app
            .world_mut()
            .spawn((Pane, PaneId("leaf-A".to_string()), ChildOf(tab)))
            .id();
        app.world_mut().spawn((Stack::default(), ChildOf(leaf)));
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z.example".to_string(),
                space_id: "s1".to_string(),
                closed_at: UnixMillis::now().0 + 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "leaf-A".to_string(),
                stack_index: 0,
                pane_path: Vec::new(),
            },
        ));
        app.dispatch_reopen();

        let children = app.world().entity(leaf).get::<Children>().unwrap();
        let stacks: Vec<Entity> = children
            .iter()
            .filter(|&e| app.world().entity(e).contains::<Stack>())
            .collect();
        assert_eq!(stacks.len(), 2, "stack added into the existing leaf pane");
        let opens = app.drain_opens();
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://z.example");
    }

    #[test]
    fn reopen_without_position_recreates_tab() {
        let mut app = Fixture::new();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "https://a.example".to_string(),
            space_id: "s1".to_string(),
            closed_at: UnixMillis::now().0 + 5,
            ..default()
        });
        app.dispatch_reopen();
        let opens = app.drain_opens();
        assert_eq!(opens.len(), 1);
        let mut tabs = app.world_mut().query::<&crate::tab::Tab>();
        assert_eq!(tabs.iter(app.world()).count(), 1, "a tab was recreated");
    }

    #[test]
    fn reopen_readds_leaf_under_surviving_split() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        app.world_mut()
            .spawn((Pane, PaneId("survivor".to_string()), ChildOf(root)));
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: UnixMillis::now().0 + 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "gone-leaf".to_string(),
                stack_index: 0,
                pane_path: vec![PaneStep {
                    split_id: "root".to_string(),
                    axis: SplitAxis::Row,
                    child_index: 1,
                    flex_weights: vec![1.0, 1.0],
                }],
            },
        ));
        app.dispatch_reopen();

        let root_children = app.world().entity(root).get::<Children>().unwrap();
        let panes: Vec<Entity> = root_children
            .iter()
            .filter(|&e| app.world().entity(e).contains::<Pane>())
            .collect();
        assert_eq!(
            panes.len(),
            2,
            "reopened leaf re-added under surviving split"
        );
        let has_stack = panes.iter().any(|&p| {
            app.world()
                .entity(p)
                .get::<Children>()
                .map(|c| c.iter().any(|e| app.world().entity(e).contains::<Stack>()))
                .unwrap_or(false)
        });
        assert!(has_stack);
        assert_eq!(app.drain_opens().len(), 1);
    }

    #[test]
    fn reopen_reconstructs_collapsed_split_level() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        app.world_mut()
            .spawn((Pane, PaneId("root-leaf".to_string()), ChildOf(root)));
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: UnixMillis::now().0 + 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "deep-leaf".to_string(),
                stack_index: 0,
                pane_path: vec![
                    PaneStep {
                        split_id: "root".to_string(),
                        axis: SplitAxis::Row,
                        child_index: 1,
                        flex_weights: vec![1.0, 1.0],
                    },
                    PaneStep {
                        split_id: "nested".to_string(),
                        axis: SplitAxis::Column,
                        child_index: 0,
                        flex_weights: vec![1.0, 1.0],
                    },
                ],
            },
        ));
        app.dispatch_reopen();

        let mut ids = app.world_mut().query::<&PaneId>();
        let recreated_nested = ids.iter(app.world()).any(|id| id.0 == "nested");
        assert!(recreated_nested, "nested split recreated by id");
        let stack_count = app.world_mut().query::<&Stack>().iter(app.world()).count();
        assert_eq!(stack_count, 1);
        assert_eq!(app.drain_opens().len(), 1);
    }

    #[test]
    fn reopen_focuses_restored_stack_and_ancestors() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let leaf = app
            .world_mut()
            .spawn((Pane, PaneId("leaf-A".to_string()), ChildOf(tab)))
            .id();
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: UnixMillis::now().0 + 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "leaf-A".to_string(),
                stack_index: 0,
                pane_path: Vec::new(),
            },
        ));
        app.dispatch_reopen();
        assert!(app.world().entity(leaf).get::<LastActivatedAt>().is_some());
        assert!(app.world().entity(tab).get::<LastActivatedAt>().is_some());
    }

    #[test]
    fn reopen_focus_propagates_through_reattached_splits() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        let mid = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                PaneId("mid".to_string()),
                ChildOf(root),
            ))
            .id();
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: UnixMillis::now().0 + 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "leaf-deep".to_string(),
                stack_index: 0,
                pane_path: vec![
                    PaneStep {
                        split_id: "root".to_string(),
                        axis: SplitAxis::Row,
                        child_index: 1,
                        flex_weights: vec![1.0, 1.0],
                    },
                    PaneStep {
                        split_id: "mid".to_string(),
                        axis: SplitAxis::Column,
                        child_index: 1,
                        flex_weights: vec![1.0, 1.0],
                    },
                ],
            },
        ));
        app.dispatch_reopen();
        assert!(
            app.world().entity(mid).get::<LastActivatedAt>().is_some(),
            "reattached intermediate split is activated through the restored chain"
        );
    }

    #[test]
    fn reopen_stale_leaf_id_that_is_now_split_uses_descendant_leaf() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        let promoted = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                PaneId("old-leaf".to_string()),
                ChildOf(root),
            ))
            .id();
        let survivor = app
            .world_mut()
            .spawn((Pane, PaneId("survivor".to_string()), ChildOf(promoted)))
            .id();
        app.world_mut().spawn((Stack::default(), ChildOf(survivor)));
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://reopened.example".to_string(),
                space_id: "s1".to_string(),
                closed_at: UnixMillis::now().0 + 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "old-leaf".to_string(),
                stack_index: 0,
                pane_path: vec![PaneStep {
                    split_id: "root".to_string(),
                    axis: SplitAxis::Row,
                    child_index: 0,
                    flex_weights: vec![1.0],
                }],
            },
        ));

        app.dispatch_reopen();

        let reopened = app
            .world_mut()
            .query::<(Entity, &PageMetadata)>()
            .iter(app.world())
            .find(|(_, metadata)| metadata.url == "https://reopened.example")
            .map(|(entity, _)| entity)
            .expect("reopened stack");
        let parent = app.world().get::<ChildOf>(reopened).unwrap().parent();
        assert_eq!(parent, survivor);
        assert!(app.world().get::<PaneSplit>(parent).is_none());
        assert!(
            app.world()
                .get::<Children>(promoted)
                .is_some_and(|children| !children.contains(&reopened))
        );
    }

    #[test]
    fn reopen_resplits_collapsed_two_pane() {
        let mut app = Fixture::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((Pane, PaneId("root".to_string()), ChildOf(tab)))
            .id();
        let survivor_stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(root)))
            .id();
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: UnixMillis::now().0 + 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "paneR".to_string(),
                stack_index: 0,
                pane_path: vec![PaneStep {
                    split_id: "root".to_string(),
                    axis: SplitAxis::Row,
                    child_index: 1,
                    flex_weights: vec![1.0, 1.0],
                }],
            },
        ));
        app.dispatch_reopen();

        assert!(
            app.world().entity(root).get::<PaneSplit>().is_some(),
            "root was re-split"
        );
        let panes: Vec<Entity> = app
            .world()
            .entity(root)
            .get::<Children>()
            .unwrap()
            .iter()
            .filter(|&e| app.world().entity(e).contains::<Pane>())
            .collect();
        assert_eq!(panes.len(), 2, "two panes under the restored split");
        let total_stacks = app.world_mut().query::<&Stack>().iter(app.world()).count();
        assert_eq!(total_stacks, 2);
        let survivor_pane = app
            .world()
            .entity(survivor_stack)
            .get::<ChildOf>()
            .unwrap()
            .parent();
        assert!(
            panes.contains(&survivor_pane),
            "survivor re-homed into a pane"
        );
        assert_eq!(app.drain_opens().len(), 1);
    }

    #[test]
    fn reopen_reconstructs_collapsed_outer_split_around_surviving_split() {
        let mut app = Fixture::closing();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let outer = app
            .world_mut()
            .spawn((
                Pane::split_bundle(PaneSplitDirection::Row),
                PaneId("outer".to_string()),
                ChildOf(tab),
            ))
            .id();
        let pane_a = app
            .world_mut()
            .spawn((Pane::bundle(), PaneId("pane-a".to_string()), ChildOf(outer)))
            .id();
        let stack_a = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://a.example".to_string(),
                    ..default()
                },
                ChildOf(pane_a),
            ))
            .id();
        let inner = app
            .world_mut()
            .spawn((
                Pane::split_bundle(PaneSplitDirection::Column),
                PaneId("inner".to_string()),
                ChildOf(outer),
            ))
            .id();
        let pane_b = app
            .world_mut()
            .spawn((Pane::bundle(), PaneId("pane-b".to_string()), ChildOf(inner)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                url: "https://b.example".to_string(),
                ..default()
            },
            ChildOf(pane_b),
        ));
        let pane_c = app
            .world_mut()
            .spawn((Pane::bundle(), PaneId("pane-c".to_string()), ChildOf(inner)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                url: "https://c.example".to_string(),
                ..default()
            },
            ChildOf(pane_c),
        ));

        app.world_mut()
            .write_message(CloseStackRequest::by_user(stack_a));
        app.update();
        app.update();

        assert_eq!(
            app.world().get::<PaneSplit>(outer).unwrap().direction,
            PaneSplitDirection::Column
        );
        let collapsed: Vec<Entity> = app
            .world()
            .get::<Children>(outer)
            .unwrap()
            .iter()
            .filter(|&entity| app.world().get::<Pane>(entity).is_some())
            .collect();
        assert_eq!(collapsed, vec![pane_b, pane_c]);

        app.dispatch_reopen();

        assert_eq!(
            app.world().get::<PaneSplit>(outer).unwrap().direction,
            PaneSplitDirection::Row
        );
        let restored: Vec<Entity> = app
            .world()
            .get::<Children>(outer)
            .unwrap()
            .iter()
            .filter(|&entity| app.world().get::<Pane>(entity).is_some())
            .collect();
        assert_eq!(restored.len(), 2);
        assert_eq!(app.world().get::<PaneId>(restored[0]).unwrap().0, "pane-a");
        assert!(app.world().get::<PaneSplit>(restored[0]).is_none());
        let surviving_split = restored[1];
        assert_eq!(
            app.world()
                .get::<PaneSplit>(surviving_split)
                .unwrap()
                .direction,
            PaneSplitDirection::Column
        );
        let surviving_children: Vec<Entity> = app
            .world()
            .get::<Children>(surviving_split)
            .unwrap()
            .iter()
            .filter(|&entity| app.world().get::<Pane>(entity).is_some())
            .collect();
        assert_eq!(surviving_children, vec![pane_b, pane_c]);
        assert_eq!(app.drain_opens().len(), 1);
    }

    #[test]
    fn reopen_inserts_directly_into_uncollapsed_three_pane_split() {
        let mut app = Fixture::closing();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let outer = app
            .world_mut()
            .spawn((
                Pane::split_bundle(PaneSplitDirection::Row),
                PaneId("outer".to_string()),
                ChildOf(tab),
            ))
            .id();
        let pane_a = app
            .world_mut()
            .spawn((Pane::bundle(), PaneId("pane-a".to_string()), ChildOf(outer)))
            .id();
        let stack_a = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://a.example".to_string(),
                    ..default()
                },
                ChildOf(pane_a),
            ))
            .id();
        let pane_b = app
            .world_mut()
            .spawn((Pane::bundle(), PaneId("pane-b".to_string()), ChildOf(outer)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                url: "https://b.example".to_string(),
                ..default()
            },
            ChildOf(pane_b),
        ));
        let pane_c = app
            .world_mut()
            .spawn((Pane::bundle(), PaneId("pane-c".to_string()), ChildOf(outer)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                url: "https://c.example".to_string(),
                ..default()
            },
            ChildOf(pane_c),
        ));

        app.world_mut()
            .write_message(CloseStackRequest::by_user(stack_a));
        app.update();
        app.update();

        assert_eq!(
            app.world().get::<PaneSplit>(outer).unwrap().direction,
            PaneSplitDirection::Row
        );
        let surviving: Vec<Entity> = app
            .world()
            .get::<Children>(outer)
            .unwrap()
            .iter()
            .filter(|&entity| app.world().get::<Pane>(entity).is_some())
            .collect();
        assert_eq!(surviving, vec![pane_b, pane_c]);

        app.dispatch_reopen();

        assert_eq!(
            app.world().get::<PaneSplit>(outer).unwrap().direction,
            PaneSplitDirection::Row
        );
        let restored: Vec<Entity> = app
            .world()
            .get::<Children>(outer)
            .unwrap()
            .iter()
            .filter(|&entity| app.world().get::<Pane>(entity).is_some())
            .collect();
        assert_eq!(restored.len(), 3);
        assert_eq!(app.world().get::<PaneId>(restored[0]).unwrap().0, "pane-a");
        assert_eq!(restored[1], pane_b);
        assert_eq!(restored[2], pane_c);
        assert_eq!(app.drain_opens().len(), 1);
    }
}
