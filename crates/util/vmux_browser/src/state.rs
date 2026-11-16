use bevy::prelude::{
    App, Changed, ChildOf, Children, Commands, Entity, Has, Name, NonSend, On, Plugin, Query, Ref,
    Remove, Res, Single, Window, With, Without,
};
use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use vmux_api::bookmark::{
    BookmarkFolderChoice, BookmarkFolderRow, BookmarkNode, BookmarkRow, BookmarkStateEvent,
    SmartBookmarkFolder,
};
use vmux_api::{PageIcon, VmuxRoute};
use vmux_ecs::{
    Active, Bookmark, BookmarkOrder, Collapsed, Folder, PageIdentity, PageMetadata, Pin,
    UiStateWrite, Uuid,
    notify::AgentDoneUnseen,
    page::{HostHistory, PageReady},
};
use vmux_git::worktree::RepoInfo;
use vmux_git::{GitDiffSource, RepoInfoCache};
use vmux_history::LastActivatedAt;
use vmux_layout::hosted_page::StackSurface;
use vmux_layout::projection::{
    BookmarkProjection, PaneTreeProjection, ProjectProjection, StackProjection,
};
use vmux_layout::{Browser, Loading};
use vmux_layout::{
    Header, LayoutCef, NavigationState, Open, UpdateState,
    event::{
        AddressParts, HEADER_HEIGHT_PX, LayoutGeometry, PaneNode, PaneTreeState, SIDE_SHEET_GAP_PX,
        StackNavigationState, StackNode, StackRow, TabBoundary, TabBoundaryState, TabListState,
        TabRow, UpdateCleared, UpdateProgress, UpdateReady,
    },
    pane::{Pane, PaneSplit, SideSheetCardCollapsed, Zoomed},
    presentation::Retiring,
    side_sheet::{SideSheet, SideSheetPosition, SideSheetSections, SideSheetSectionsExpanded},
    space::{CurrentSpace, Space, SpaceId},
    stack::{ActiveTabParam, FocusedStack, LayoutFocus, Stack},
    state::LayoutUiState,
    tab::Tab,
    toggle::{LayoutHidden, UrlBarHidden},
    window::{FocusedWindow, VmuxWindow, WindowHierarchy},
};

use vmux_setting::AppSettings;
use vmux_space::{ExpandedProjectDirs, SpaceProjects};

use crate::host::{LayoutFixedOffsets, LayoutWindowPadding};
use vmux_flex::prelude::*;

pub(crate) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(retire_projection_receipts)
            .add_observer(retire_window_geometry)
            .add_observer(retire_window_focus)
            .add_observer(retire_update_notice)
            .add_systems(Startup, spawn_revision)
            .add_systems(
                PostUpdate,
                mark_page_dirty.after(vmux_layout::presentation::PresentationFlushSet),
            )
            .add_systems(
                PostUpdate,
                (
                    push_stacks_host_emit,
                    push_pane_tree_emit,
                    push_tabs_host_emit,
                    push_bookmarks_host_emit,
                    push_update_notice_emit,
                    push_projects_host_emit,
                )
                    .in_set(vmux_layout::projection::LayoutUiProjectionSet::Source)
                    .after(mark_page_dirty),
            )
            .add_systems(
                PostUpdate,
                (mark_computed_layout_dirty, push_layout_emit)
                    .chain()
                    .before(vmux_ecs::UiStateFlushSet)
                    .after(LayoutSystems::Layout)
                    .after(mark_page_dirty),
            );
    }
}

pub(crate) struct PagePresentation;

impl PagePresentation {
    pub(crate) fn title(metadata: &PageMetadata, identity: Option<&PageIdentity>) -> String {
        match identity.and_then(|identity| identity.title.as_deref()) {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => metadata.title.clone(),
        }
    }

    fn metadata<'a>(
        pending: Option<&'a PageMetadata>,
        presented: Option<&'a PageMetadata>,
    ) -> Option<&'a PageMetadata> {
        pending
            .filter(|metadata| !metadata.url.is_empty())
            .or(presented)
    }

    fn identity<'a>(
        metadata: &PageMetadata,
        presented: Option<&PageMetadata>,
        identity: Option<&'a PageIdentity>,
    ) -> Option<&'a PageIdentity> {
        presented
            .is_some_and(|presented| presented.url == metadata.url)
            .then_some(identity)
            .flatten()
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct TabProjectionData<'w, 's> {
    tabs: Query<'w, 's, (Entity, &'static Tab, &'static LastActivatedAt), Without<Retiring>>,
    hierarchy: vmux_layout::tab::TabHierarchy<'w, 's>,
    active_tab: ActiveTabParam<'w, 's>,
    focus: LayoutFocus<'w, 's>,
    stack_timestamps:
        Query<'w, 's, (Entity, &'static LastActivatedAt), (With<Stack>, Without<Retiring>)>,
    surfaces: StackSurface<'w, 's>,
    stack_metadata: Query<'w, 's, &'static PageMetadata, With<Stack>>,
    browser_metadata: Query<
        'w,
        's,
        (
            &'static PageMetadata,
            Option<&'static PageIdentity>,
            &'static ChildOf,
        ),
        With<Browser>,
    >,
    done_agents: Query<'w, 's, Entity, With<AgentDoneUnseen>>,
}

impl TabProjectionData<'_, '_> {
    fn active_stack(&self, tab: Entity) -> Option<Entity> {
        let mut selected: Option<(Entity, i64)> = None;
        for pane in self.focus.leaves(tab) {
            let Some(stack) = self.focus.stack(pane) else {
                continue;
            };
            let Ok((entity, timestamp)) = self.stack_timestamps.get(stack) else {
                continue;
            };
            if selected.is_none_or(|(_, previous)| timestamp.0 >= previous) {
                selected = Some((entity, timestamp.0));
            }
        }
        selected.map(|(entity, _)| entity)
    }

    fn page(&self, stack: Entity) -> Option<(PageMetadata, Option<&PageIdentity>)> {
        let presented = self
            .surfaces
            .current(stack)
            .and_then(|surface| self.browser_metadata.get(surface).ok())
            .filter(|(_, _, parent)| parent.parent() == stack);
        let pending = self.stack_metadata.get(stack).ok();
        let metadata =
            PagePresentation::metadata(pending, presented.map(|(metadata, _, _)| metadata))?;
        let identity = PagePresentation::identity(
            metadata,
            presented.map(|(metadata, _, _)| metadata),
            presented.and_then(|(_, identity, _)| identity),
        );
        Some((metadata.clone(), identity))
    }

    fn rows(&self) -> Vec<TabRow> {
        let active_tab = self.active_tab.get();
        let done_tabs = self
            .done_agents
            .iter()
            .filter_map(|agent| self.hierarchy.entity(agent))
            .collect::<std::collections::HashSet<_>>();
        let ordered = match active_tab {
            Some(anchor) => self.hierarchy.siblings(anchor),
            None => Vec::new(),
        };
        let mut rows = Vec::new();
        for entity in ordered {
            let Ok((entity, tab, _)) = self.tabs.get(entity) else {
                continue;
            };
            let page = self.active_stack(entity).and_then(|stack| self.page(stack));
            let (title, url, icon, bg_color) = match page {
                Some((metadata, identity)) => (
                    PagePresentation::title(&metadata, identity),
                    metadata.url,
                    metadata.icon,
                    metadata.bg_color,
                ),
                None => Default::default(),
            };
            rows.push(TabRow {
                id: entity.to_bits().to_string(),
                name: if tab.name.is_empty() {
                    "Tab".to_string()
                } else {
                    tab.name.clone()
                },
                is_active: Some(entity) == active_tab,
                bg_color,
                title,
                url,
                icon,
                is_done_unseen: done_tabs.contains(&entity),
            });
        }
        rows
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct FocusedLayout<'w, 's> {
    focused: FocusedWindow<'w, 's>,
    layouts: Query<'w, 's, (Entity, Ref<'static, PageReady>), With<LayoutCef>>,
    hierarchy: WindowHierarchy<'w, 's>,
}

impl FocusedLayout<'_, '_> {
    fn get(&self) -> Option<(Entity, bool)> {
        let entity = if self.hierarchy.is_empty() {
            self.layouts.iter().next().map(|(entity, _)| entity)
        } else if let Some(window) = self.focused.entity() {
            self.layouts.iter().find_map(|(entity, _)| {
                (self.hierarchy.get(entity) == Some(window)).then_some(entity)
            })
        } else {
            None
        }?;
        let (_, ready) = self.layouts.get(entity).ok()?;
        Some((entity, ready.is_changed()))
    }
}

fn spawn_revision(mut commands: Commands) {
    commands.spawn((
        Name::new("Page state revision"),
        StateRevision::default(),
        LayoutGeometryRevision::default(),
    ));
}

#[derive(Component, Default)]
struct StateRevision(u64);

#[derive(Component, Default)]
struct LayoutGeometryRevision(u64);

impl StateRevision {
    fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

impl LayoutGeometryRevision {
    fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

#[derive(Component)]
struct ProjectionReceipt<T: Send + Sync + 'static> {
    revision: u64,
    body: String,
    payload: std::marker::PhantomData<fn() -> T>,
}

struct BookmarkFolders(Vec<BookmarkFolderChoice>);

impl BookmarkFolders {
    fn from_nodes(nodes: &[BookmarkNode]) -> Self {
        let folders = nodes
            .iter()
            .filter_map(|node| match node {
                BookmarkNode::Folder(folder) => Some(folder.clone()),
                BookmarkNode::Entry(_) => None,
            })
            .collect::<Vec<_>>();
        let mut output = Vec::new();
        Self::collect(
            &folders,
            None,
            "",
            &[],
            &mut std::collections::HashSet::new(),
            &mut output,
        );
        Self(output)
    }

    fn collect(
        folders: &[BookmarkFolderRow],
        parent: Option<&str>,
        parent_label: &str,
        ancestors: &[String],
        visited: &mut std::collections::HashSet<String>,
        output: &mut Vec<BookmarkFolderChoice>,
    ) {
        for folder in folders
            .iter()
            .filter(|folder| folder.parent.as_deref() == parent)
        {
            if !visited.insert(folder.uuid.clone()) {
                continue;
            }
            let label = match parent_label.is_empty() {
                true => folder.name.clone(),
                false => format!("{parent_label} / {}", folder.name),
            };
            output.push(BookmarkFolderChoice {
                uuid: folder.uuid.clone(),
                label: label.clone(),
                ancestors: ancestors.to_vec(),
            });
            let mut child_ancestors = ancestors.to_vec();
            child_ancestors.push(folder.uuid.clone());
            Self::collect(
                folders,
                Some(&folder.uuid),
                &label,
                &child_ancestors,
                visited,
                output,
            );
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct LayoutProjection<'w, 's, T: Send + Sync + 'static> {
    commands: Commands<'w, 's>,
    delivery: LayoutDelivery<'w, 's>,
    layout: FocusedLayout<'w, 's>,
    revision: Single<'w, 's, &'static StateRevision>,
    receipts: Query<
        'w,
        's,
        (
            Ref<'static, LayoutCef>,
            Option<&'static ProjectionReceipt<T>>,
        ),
    >,
}

#[derive(Clone, Copy)]
struct ProjectionTarget {
    entity: Entity,
    revision: u64,
    page_ready_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectProjectionSelection {
    tab: Entity,
    revision: u64,
}

#[derive(Component)]
struct ProjectSelectionReceipt(Option<ProjectProjectionSelection>);

impl<T: Send + Sync + 'static> LayoutProjection<'_, '_, T> {
    fn target(&self) -> Option<ProjectionTarget> {
        self.target_for_revision(self.revision.0, false)
    }

    fn target_for(&self, force: bool) -> Option<ProjectionTarget> {
        self.target_for_revision(self.revision.0, force)
    }

    fn target_for_revision(&self, revision: u64, force: bool) -> Option<ProjectionTarget> {
        let (entity, ready_changed) = self.layout.get()?;
        if !self.delivery.available(entity) {
            return None;
        }
        let (owner, receipt) = self.receipts.get(entity).ok()?;
        let page_ready_changed = ready_changed || owner.is_added();
        if !force
            && !page_ready_changed
            && receipt.is_some_and(|receipt| receipt.revision == revision)
        {
            return None;
        }
        Some(ProjectionTarget {
            entity,
            revision,
            page_ready_changed,
        })
    }

    fn should_emit(&mut self, target: ProjectionTarget, body: String) -> bool {
        let previous = self
            .receipts
            .get(target.entity)
            .ok()
            .and_then(|(_, receipt)| receipt);
        let changed =
            target.page_ready_changed || previous.is_none_or(|previous| previous.body != body);
        self.commands
            .entity(target.entity)
            .insert(ProjectionReceipt::<T> {
                revision: target.revision,
                body,
                payload: std::marker::PhantomData,
            });
        changed
    }
}

fn retire_projection_receipts(removed: On<Remove, LayoutCef>, mut commands: Commands) {
    commands.entity(removed.event_target()).try_remove::<(
        ProjectionReceipt<LayoutGeometry>,
        ProjectionReceipt<StackNavigationState>,
        ProjectionReceipt<PaneTreeState>,
        ProjectionReceipt<TabBoundaryState>,
        ProjectionReceipt<BookmarkStateEvent>,
        ProjectionReceipt<TabListState>,
        ProjectSelectionReceipt,
    )>();
}

type PageProjectionChanged = Or<(
    Changed<vmux_layout::page_slot::Current>,
    Changed<PageMetadata>,
    Changed<PageIdentity>,
    Changed<NavigationState>,
    Changed<HostHistory>,
    Changed<Loading>,
    Changed<GitDiffSource>,
    Changed<AgentDoneUnseen>,
)>;

type LayoutProjectionChanged = Or<(
    Changed<ChildOf>,
    Changed<Children>,
    Changed<LastActivatedAt>,
    Changed<Open>,
    Changed<SideSheetPosition>,
    Changed<SideSheetCardCollapsed>,
    Changed<Tab>,
    Changed<HostWindow>,
    Changed<SpaceId>,
    Changed<SideSheetSectionsExpanded>,
)>;

type LayoutGeometryChanged = Or<(
    Changed<Node>,
    Changed<Open>,
    Changed<SideSheetPosition>,
    Changed<HostWindow>,
    Changed<LayoutHidden>,
    Changed<UrlBarHidden>,
)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowGeometryState {
    width: u32,
    height: u32,
    scale: u32,
}

impl WindowGeometryState {
    fn of(window: &Window) -> Self {
        Self {
            width: window.resolution.physical_width(),
            height: window.resolution.physical_height(),
            scale: window.resolution.scale_factor().to_bits(),
        }
    }
}

#[derive(Component)]
struct WindowGeometryReceipt(WindowGeometryState);

#[derive(Component)]
struct WindowFocusReceipt(Option<Entity>);

fn retire_window_geometry(removed: On<Remove, Window>, mut commands: Commands) {
    commands
        .entity(removed.event_target())
        .try_remove::<WindowGeometryReceipt>();
}

fn retire_window_focus(removed: On<Remove, StateRevision>, mut commands: Commands) {
    commands
        .entity(removed.event_target())
        .try_remove::<WindowFocusReceipt>();
}

type LayoutMarkerChanged = Or<(
    Changed<Browser>,
    Changed<Header>,
    Changed<LayoutCef>,
    Changed<PageReady>,
    Changed<SideSheet>,
    Changed<VmuxWindow>,
    Changed<Stack>,
    Changed<Pane>,
    Changed<PaneSplit>,
    Changed<Zoomed>,
    Changed<Space>,
    Changed<CurrentSpace>,
    Changed<Active>,
    Or<(Changed<LayoutHidden>, Changed<UrlBarHidden>)>,
    Changed<Retiring>,
)>;

type BookmarkProjectionChanged = Or<(
    Changed<Uuid>,
    Changed<BookmarkOrder>,
    Changed<Bookmark>,
    Changed<Pin>,
    Changed<Folder>,
    Changed<Name>,
    Changed<Collapsed>,
    Changed<SmartBookmarkFolder>,
)>;

#[derive(bevy::ecs::system::SystemParam)]
struct StateChanges<'w, 's> {
    pages: Query<'w, 's, (), PageProjectionChanged>,
    layout: Query<'w, 's, (), LayoutProjectionChanged>,
    markers: Query<'w, 's, (), LayoutMarkerChanged>,
    bookmarks: Query<'w, 's, (), BookmarkProjectionChanged>,
    projects: Query<'w, 's, (), Changed<ExpandedProjectDirs>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct LayoutGeometryChanges<'w, 's> {
    changed: Query<'w, 's, (), LayoutGeometryChanged>,
}

impl LayoutGeometryChanges<'_, '_> {
    fn any(&self) -> bool {
        !self.changed.is_empty()
    }
}

impl StateChanges<'_, '_> {
    fn any(&self) -> bool {
        !self.pages.is_empty()
            || !self.layout.is_empty()
            || !self.markers.is_empty()
            || !self.bookmarks.is_empty()
            || !self.projects.is_empty()
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PageStateRemovals<'w, 's> {
    current: RemovedComponents<'w, 's, vmux_layout::page_slot::Current>,
    browser: RemovedComponents<'w, 's, Browser>,
    child_of: RemovedComponents<'w, 's, ChildOf>,
    children: RemovedComponents<'w, 's, Children>,
    page_metadata: RemovedComponents<'w, 's, PageMetadata>,
    page_identity: RemovedComponents<'w, 's, PageIdentity>,
    page_ready: RemovedComponents<'w, 's, PageReady>,
    navigation: RemovedComponents<'w, 's, NavigationState>,
    history: RemovedComponents<'w, 's, HostHistory>,
    loading: RemovedComponents<'w, 's, Loading>,
    git_diff: RemovedComponents<'w, 's, GitDiffSource>,
    done: RemovedComponents<'w, 's, AgentDoneUnseen>,
    active: RemovedComponents<'w, 's, Active>,
    header: RemovedComponents<'w, 's, Header>,
    open: RemovedComponents<'w, 's, Open>,
    side_sheet: RemovedComponents<'w, 's, SideSheet>,
    side_sheet_position: RemovedComponents<'w, 's, SideSheetPosition>,
    collapsed_pane: RemovedComponents<'w, 's, SideSheetCardCollapsed>,
    vmux_window: RemovedComponents<'w, 's, VmuxWindow>,
    window: RemovedComponents<'w, 's, Window>,
    node: RemovedComponents<'w, 's, Node>,
    layout_cef: RemovedComponents<'w, 's, LayoutCef>,
    host_window: RemovedComponents<'w, 's, HostWindow>,
    tab: RemovedComponents<'w, 's, Tab>,
    stack: RemovedComponents<'w, 's, Stack>,
    last_activated: RemovedComponents<'w, 's, LastActivatedAt>,
    pane: RemovedComponents<'w, 's, Pane>,
    pane_split: RemovedComponents<'w, 's, PaneSplit>,
    zoomed: RemovedComponents<'w, 's, Zoomed>,
    space: RemovedComponents<'w, 's, Space>,
    space_id: RemovedComponents<'w, 's, SpaceId>,
    current_space: RemovedComponents<'w, 's, CurrentSpace>,
    sections_expanded: RemovedComponents<'w, 's, SideSheetSectionsExpanded>,
    uuid: RemovedComponents<'w, 's, Uuid>,
    bookmark_order: RemovedComponents<'w, 's, BookmarkOrder>,
    bookmark: RemovedComponents<'w, 's, Bookmark>,
    pin: RemovedComponents<'w, 's, Pin>,
    folder: RemovedComponents<'w, 's, Folder>,
    name: RemovedComponents<'w, 's, Name>,
    collapsed: RemovedComponents<'w, 's, Collapsed>,
    smart_folder: RemovedComponents<'w, 's, SmartBookmarkFolder>,
    expanded_projects: RemovedComponents<'w, 's, ExpandedProjectDirs>,
    layout_hidden: RemovedComponents<'w, 's, LayoutHidden>,
    url_bar_hidden: RemovedComponents<'w, 's, UrlBarHidden>,
    retiring: RemovedComponents<'w, 's, Retiring>,
}

impl PageStateRemovals<'_, '_> {
    fn changes(&mut self) -> PageStateRemovalChanges {
        let header = self.header.read().count() > 0;
        let open = self.open.read().count() > 0;
        let side_sheet = self.side_sheet.read().count() > 0;
        let side_sheet_position = self.side_sheet_position.read().count() > 0;
        let vmux_window = self.vmux_window.read().count() > 0;
        let window = self.window.read().count() > 0;
        let node = self.node.read().count() > 0;
        let host_window = self.host_window.read().count() > 0;
        let layout_hidden = self.layout_hidden.read().count() > 0;
        let url_bar_hidden = self.url_bar_hidden.read().count() > 0;
        let geometry = header
            || open
            || side_sheet
            || side_sheet_position
            || vmux_window
            || window
            || node
            || host_window
            || layout_hidden
            || url_bar_hidden;
        let mut state = geometry;
        state |= self.current.read().count() > 0;
        state |= self.browser.read().count() > 0;
        state |= self.child_of.read().count() > 0;
        state |= self.children.read().count() > 0;
        state |= self.page_metadata.read().count() > 0;
        state |= self.page_identity.read().count() > 0;
        state |= self.page_ready.read().count() > 0;
        state |= self.navigation.read().count() > 0;
        state |= self.history.read().count() > 0;
        state |= self.loading.read().count() > 0;
        state |= self.git_diff.read().count() > 0;
        state |= self.done.read().count() > 0;
        state |= self.active.read().count() > 0;
        state |= self.collapsed_pane.read().count() > 0;
        state |= self.layout_cef.read().count() > 0;
        state |= self.tab.read().count() > 0;
        state |= self.stack.read().count() > 0;
        state |= self.last_activated.read().count() > 0;
        state |= self.pane.read().count() > 0;
        state |= self.pane_split.read().count() > 0;
        state |= self.zoomed.read().count() > 0;
        state |= self.space.read().count() > 0;
        state |= self.space_id.read().count() > 0;
        state |= self.current_space.read().count() > 0;
        state |= self.sections_expanded.read().count() > 0;
        state |= self.uuid.read().count() > 0;
        state |= self.bookmark_order.read().count() > 0;
        state |= self.bookmark.read().count() > 0;
        state |= self.pin.read().count() > 0;
        state |= self.folder.read().count() > 0;
        state |= self.name.read().count() > 0;
        state |= self.collapsed.read().count() > 0;
        state |= self.smart_folder.read().count() > 0;
        state |= self.expanded_projects.read().count() > 0;
        state |= self.retiring.read().count() > 0;
        PageStateRemovalChanges { state, geometry }
    }
}

struct PageStateRemovalChanges {
    state: bool,
    geometry: bool,
}

fn mark_page_dirty(
    changes: StateChanges,
    geometry: LayoutGeometryChanges,
    mut removals: PageStateRemovals,
    focused_window: FocusedWindow,
    focused_stack: FocusedStack,
    settings: Res<AppSettings>,
    repo_info: Option<Single<Ref<RepoInfoCache>>>,
    windows: Query<
        (Entity, Ref<Window>, Option<&WindowGeometryReceipt>),
        Or<(Changed<Window>, Without<WindowGeometryReceipt>)>,
    >,
    mut state: Single<(Entity, &mut StateRevision, Option<&WindowFocusReceipt>)>,
    mut geometry_revision: Single<&mut LayoutGeometryRevision>,
    mut commands: Commands,
) {
    let removals = removals.changes();
    let current = focused_window.entity();
    let (owner, revision, focus) = &mut *state;
    let previous = focus.and_then(|focus| focus.0);
    let focused_window_changed = previous != current;
    if focus.is_none() || focused_window_changed {
        commands.entity(*owner).insert(WindowFocusReceipt(current));
    }
    let mut window_geometry_changed = false;
    for (entity, window, previous) in &windows {
        let next = WindowGeometryState::of(&window);
        if window.is_added() || previous.is_none_or(|previous| previous.0 != next) {
            window_geometry_changed = true;
            commands.entity(entity).insert(WindowGeometryReceipt(next));
        }
    }
    let resource_changed = focused_window_changed
        || focused_stack.is_changed()
        || settings.is_changed()
        || repo_info.as_ref().is_some_and(|value| value.is_changed());
    if resource_changed || changes.any() || removals.state {
        revision.advance();
    }
    if focused_window_changed
        || settings.is_changed()
        || window_geometry_changed
        || geometry.any()
        || removals.geometry
    {
        geometry_revision.advance();
    }
}

fn mark_computed_layout_dirty(
    headers: Query<(), (With<Header>, Changed<ComputedNode>)>,
    mut geometry_revision: Single<&mut LayoutGeometryRevision>,
) {
    if !headers.is_empty() {
        geometry_revision.advance();
    }
}

fn push_layout_emit(
    mut projection: LayoutProjection<LayoutGeometry>,
    geometry_revision: Single<&LayoutGeometryRevision>,
    header_q: Query<(Entity, Has<Open>, Option<&ComputedNode>), With<Header>>,
    side_sheet_q: Query<(Entity, &SideSheetPosition, Has<Open>, &Node), With<SideSheet>>,
    window_q: Query<(&HostWindow, &Node), With<VmuxWindow>>,
    windows: Query<(&Window, Has<LayoutHidden>, Has<UrlBarHidden>)>,
    settings: Res<AppSettings>,
) {
    let Some(target) = projection.target_for_revision(geometry_revision.0, false) else {
        return;
    };
    let cef_e = target.entity;
    let Some(host_window) = projection.layout.hierarchy.get(cef_e) else {
        return;
    };
    let window_padding = window_q
        .iter()
        .find_map(|(host, node)| (host.0 == host_window).then_some(node))
        .map(LayoutWindowPadding::from_node)
        .unwrap_or_else(|| LayoutWindowPadding::from_settings(&settings));
    let header_open = header_q.iter().any(|(entity, is_open, _)| {
        is_open && projection.layout.hierarchy.get(entity) == Some(host_window)
    });
    let (window_width_px, zen, url_bar_hidden) = windows
        .get(host_window)
        .ok()
        .map(|(window, zen, url)| (window.resolution.physical_width() as f32, zen, url))
        .unwrap_or((0.0, false, false));
    let header_offsets = header_q.iter().find_map(|(entity, _, computed)| {
        if projection.layout.hierarchy.get(entity) == Some(host_window) {
            LayoutFixedOffsets::from_node(computed?, window_width_px)
        } else {
            None
        }
    });

    let mut side_sheet_open = false;
    let mut side_sheet_width =
        vmux_layout::event::SideSheetResizeEvent::live(settings.layout.side_sheet.width).clamped();
    for (entity, position, is_open, node) in &side_sheet_q {
        if *position != SideSheetPosition::Left
            || projection.layout.hierarchy.get(entity) != Some(host_window)
        {
            continue;
        }
        side_sheet_open = is_open;
        if let Val::Px(width) = node.width {
            side_sheet_width = width;
        }
        break;
    }

    let pane_gap = window_q
        .iter()
        .find(|(host, _)| host.0 == host_window)
        .and_then(|(_, node)| match node.column_gap {
            Val::Px(gap) => Some(gap),
            _ => None,
        })
        .unwrap_or(SIDE_SHEET_GAP_PX);
    let payload = LayoutGeometry {
        zen,
        url_bar_hidden,
        header_open,
        side_sheet_open,
        header_height: header_offsets
            .map(|offsets| offsets.height)
            .unwrap_or(HEADER_HEIGHT_PX),
        side_sheet_width,
        side_sheet_content_width: vmux_layout::event::SideSheetResizeEvent::live(
            settings.layout.side_sheet.width,
        )
        .clamped(),
        pane_gap,
        radius: settings.layout.radius,
        header_left: header_offsets.map(|offsets| offsets.left),
        header_top: header_offsets.map(|offsets| offsets.top),
        header_right: header_offsets.map(|offsets| offsets.right),
        window_pad_top: window_padding.top,
        window_pad_right: window_padding.right,
        window_pad_bottom: window_padding.bottom,
        window_pad_left: window_padding.left,
    };

    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, body) {
        return;
    }
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &payload));
}

#[derive(bevy::ecs::system::SystemParam)]
struct AddressRoots<'w, 's> {
    repos: Option<Single<'w, 's, &'static mut RepoInfoCache>>,
}

impl AddressRoots<'_, '_> {
    fn resolve(&mut self, url: &str, title: &str) -> AddressParts {
        let Some(path) = vmux_path::FileUrl::parse(url).and_then(|url| url.path()) else {
            return match VmuxRoute::parse(url).is_some() {
                true => AddressParts::internal(url),
                false => AddressParts::web(url, title),
            };
        };
        if let Some(info) = self.checkout_of(&path) {
            return AddressParts::in_repo(&path, &info.repo_root, &info.name, &info.branch);
        }
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        AddressParts::on_disk(&path, &home)
    }

    fn checkout_of(&mut self, path: &std::path::Path) -> Option<RepoInfo> {
        let dir = match path.is_dir() {
            true => path,
            false => path.parent()?,
        };
        self.repos.as_mut()?.bypass_change_detection().snapshot(dir)
    }

    fn row(
        &mut self,
        metadata: &PageMetadata,
        identity: Option<&PageIdentity>,
        active: bool,
    ) -> StackRow {
        let title = PagePresentation::title(metadata, identity);
        StackRow {
            address: self.resolve(&metadata.url, &title),
            title,
            url: metadata.url.clone(),
            icon: metadata.icon.clone(),
            is_active: active,
            bg_color: metadata.bg_color.clone(),
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct NavigationRows<'w, 's> {
    browsers: Query<
        'w,
        's,
        (
            Entity,
            &'static PageMetadata,
            &'static ChildOf,
            Option<&'static NavigationState>,
            Option<&'static HostHistory>,
            Option<&'static PageIdentity>,
        ),
        With<Browser>,
    >,
    stacks: Query<
        'w,
        's,
        (&'static PageMetadata, Option<&'static HostHistory>),
        (With<Stack>, Without<Retiring>),
    >,
    zoomed: Query<'w, 's, (), With<Zoomed>>,
    focus: FocusedStack<'w, 's>,
    surfaces: StackSurface<'w, 's>,
    retiring: Query<'w, 's, (), With<Retiring>>,
    parents: Query<'w, 's, &'static ChildOf>,
    roots: AddressRoots<'w, 's>,
}

impl NavigationRows<'_, '_> {
    fn snapshot(&mut self) -> StackNavigationState {
        let active = self
            .focus
            .stack
            .filter(|stack| self.stacks.contains(*stack));
        let mut rows = Vec::new();
        let mut can_go_back = false;
        let mut can_go_forward = false;
        let mut found = false;
        if let Some(active_stack) = active {
            for (browser, metadata, parent, navigation, history, identity) in &self.browsers {
                let stack = parent.get();
                if self.retiring.contains(stack) || !self.surfaces.is_current(stack, browser) {
                    continue;
                }
                let pane = self.parents.get(stack).ok().map(|parent| parent.get());
                if pane != self.focus.pane {
                    continue;
                }
                let is_active = stack == active_stack;
                if is_active {
                    found = true;
                    if let Ok((metadata, Some(history))) = self.stacks.get(stack)
                        && VmuxRoute::parse(&metadata.url).is_some()
                    {
                        can_go_back = history.can_go_back();
                        can_go_forward = history.can_go_forward();
                    } else if let Some(history) = history {
                        can_go_back = history.can_go_back();
                        can_go_forward = history.can_go_forward();
                    } else if let Some(navigation) = navigation {
                        can_go_back = navigation.can_go_back;
                        can_go_forward = navigation.can_go_forward;
                    }
                }
                let pending = if is_active {
                    self.stacks.get(stack).ok().map(|(metadata, _)| metadata)
                } else {
                    None
                };
                let projected =
                    PagePresentation::metadata(pending, Some(metadata)).unwrap_or(metadata);
                let identity = PagePresentation::identity(projected, Some(metadata), identity);
                rows.push(self.roots.row(projected, identity, is_active));
            }
            if !found
                && let Ok((metadata, history)) = self.stacks.get(active_stack)
                && !metadata.url.is_empty()
            {
                if let Some(history) = history
                    && VmuxRoute::parse(&metadata.url).is_some()
                {
                    can_go_back = history.can_go_back();
                    can_go_forward = history.can_go_forward();
                }
                rows.push(self.roots.row(metadata, None, true));
            }
        }
        StackNavigationState {
            stacks: rows,
            can_go_back,
            can_go_forward,
            is_zoomed: self.focus.tab.is_some_and(|tab| self.zoomed.contains(tab)),
        }
    }
}

fn push_stacks_host_emit(
    mut projection: LayoutProjection<StackNavigationState>,
    mut rows: NavigationRows,
) {
    let Some(target) = projection.target() else {
        return;
    };
    let payload = rows.snapshot();
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, body) {
        return;
    }
    projection
        .commands
        .entity(target.entity)
        .insert(StackProjection(payload.clone()));
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(
            target.entity,
            &payload,
        ));
}

#[derive(bevy::ecs::system::SystemParam)]
struct PaneRows<'w, 's> {
    focus: FocusedStack<'w, 's>,
    layout: LayoutFocus<'w, 's>,
    tabs: Query<'w, 's, (), With<Tab>>,
    sections: SideSheetSections<'w, 's>,
    collapsed: Query<'w, 's, (), With<SideSheetCardCollapsed>>,
    stacks: Query<'w, 's, Entity, (With<Stack>, Without<Retiring>)>,
    metadata: Query<'w, 's, &'static PageMetadata, (With<Stack>, Without<Retiring>)>,
    children: Query<'w, 's, &'static Children>,
    surfaces: StackSurface<'w, 's>,
    browsers: Query<
        'w,
        's,
        (
            &'static PageMetadata,
            Has<Loading>,
            Option<&'static PageIdentity>,
            Option<&'static GitDiffSource>,
            Has<vmux_ecs::team::Agent>,
        ),
        With<Browser>,
    >,
}

impl PaneRows<'_, '_> {
    fn snapshot(&self) -> Option<PaneTreeState> {
        let tab = self.focus.tab?;
        if !self.tabs.contains(tab) {
            return None;
        }
        let sections = self.sections.under(tab);
        let mut panes = Vec::new();
        for pane in self.layout.leaves(tab) {
            let mut stacks = Vec::new();
            let active = self.layout.stack(pane);
            if let Ok(children) = self.children.get(pane) {
                for stack in children.iter() {
                    if self.stacks.contains(stack) {
                        stacks.push(self.stack(stack, active == Some(stack)));
                    }
                }
            }
            panes.push(PaneNode {
                id: pane.to_bits(),
                is_active: self.focus.pane == Some(pane),
                collapsed: self.collapsed.contains(pane),
                bookmarks_expanded: sections.bookmarks,
                stacks,
            });
        }
        Some(PaneTreeState { panes })
    }

    fn stack(&self, stack: Entity, active: bool) -> StackNode {
        let pending = self
            .metadata
            .get(stack)
            .ok()
            .filter(|metadata| !metadata.url.is_empty());
        if let Some(browser) = self.surfaces.current(stack)
            && self
                .children
                .get(stack)
                .is_ok_and(|children| children.contains(&browser))
            && let Ok((metadata, loading, identity, diff, agent)) = self.browsers.get(browser)
        {
            let projected = PagePresentation::metadata(pending, Some(metadata)).unwrap_or(metadata);
            let same_page = projected.url == metadata.url;
            return StackNode {
                id: stack.to_bits(),
                agent_id: (agent && same_page).then(|| browser.to_bits().to_string()),
                title: PagePresentation::title(projected, if same_page { identity } else { None }),
                url: projected.url.clone(),
                icon: projected.icon.clone(),
                is_active: active,
                is_loading: loading || !same_page,
                is_dirty: same_page && diff.is_some_and(|source| source.dirty),
                bg_color: projected.bg_color.clone(),
            };
        }
        match pending {
            Some(metadata) => StackNode {
                id: stack.to_bits(),
                agent_id: None,
                title: metadata.title.clone(),
                url: metadata.url.clone(),
                icon: metadata.icon.clone(),
                is_active: active,
                is_loading: active,
                is_dirty: false,
                bg_color: metadata.bg_color.clone(),
            },
            None => StackNode {
                id: stack.to_bits(),
                agent_id: None,
                title: "New Stack".to_string(),
                url: String::new(),
                icon: PageIcon::None,
                is_active: active,
                is_loading: false,
                is_dirty: false,
                bg_color: None,
            },
        }
    }
}

fn push_pane_tree_emit(mut projection: LayoutProjection<PaneTreeState>, rows: PaneRows) {
    let Some(target) = projection.target() else {
        return;
    };
    let Some(payload) = rows.snapshot() else {
        return;
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, body) {
        return;
    }
    projection
        .commands
        .entity(target.entity)
        .insert(PaneTreeProjection(payload.clone()));
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(
            target.entity,
            &payload,
        ));
}

#[derive(bevy::ecs::system::SystemParam)]
struct ProjectRows<'w, 's> {
    focus: FocusedStack<'w, 's>,
    projects: SpaceProjects<'w, 's>,
    active: Res<'w, vmux_space::ActiveProjectRows>,
    pending: Query<'w, 's, &'static vmux_layout::page_context::PendingWorkspaceSelection>,
    worktrees: Query<'w, 's, &'static vmux_layout::tab::TabWorktree>,
    repos: Option<Single<'w, 's, &'static mut RepoInfoCache>>,
}

impl ProjectRows<'_, '_> {
    fn selection(&self) -> Option<ProjectProjectionSelection> {
        let tab = self.focus.tab?;
        let pending = self.pending.get(tab).ok()?;
        Some(ProjectProjectionSelection {
            tab,
            revision: pending.revision(),
        })
    }

    fn snapshot(&mut self, layout: Entity) -> TabBoundaryState {
        let source = self.projects.active_rows();
        let mut projects = self
            .active
            .layout_rows(self.projects.active_space_id(), &source);
        for row in &mut projects {
            let path = if row.display_path.is_empty() {
                &row.path
            } else {
                &row.display_path
            };
            row.display_path = vmux_path::DisplayPath::project(path);
        }
        let mut boundary = projects
            .iter()
            .find(|row| row.depth == 0 && row.is_active)
            .map(|row| TabBoundary {
                effective_dir: row.path.clone(),
                project_dir: row.path.clone(),
                source: "project".to_string(),
                branch: row.branch.clone(),
                ..Default::default()
            });
        let mut branches = Vec::new();
        if let Some(cache) = self.repos.as_mut() {
            let cache = cache.bypass_change_detection();
            cache.release(layout);
            for row in &mut projects {
                if row.missing
                    || row.kind != vmux_api::space::ProjectRowKind::Project
                    || !row.is_active
                {
                    continue;
                }
                let path = std::path::Path::new(&row.path);
                let Some(info) = cache.observe_active(layout, path) else {
                    continue;
                };
                row.branch.clone_from(&info.branch);
                if let Some(project_boundary) = Self::git_boundary(&info, row, &mut branches) {
                    boundary = Some(project_boundary);
                }
            }
        }
        self.override_branch(&mut projects, &mut boundary);
        TabBoundaryState {
            boundary,
            projects,
            branches,
        }
    }

    fn git_boundary(
        info: &RepoInfo,
        row: &vmux_api::space::ProjectRow,
        branches: &mut Vec<vmux_layout::event::WorkspaceBranch>,
    ) -> Option<TabBoundary> {
        let path = std::path::Path::new(&row.path);
        let relative = path.strip_prefix(&info.repo_root).ok()?;
        let project_dir = info.project_root().join(relative);
        branches.reserve(info.branches.len());
        for holder in info.branches.iter() {
            let checkout = holder
                .checkout
                .as_ref()
                .map(|checkout| checkout.join(relative).to_string_lossy().into_owned())
                .unwrap_or_default();
            branches.push(vmux_layout::event::WorkspaceBranch {
                branch: holder.branch.clone(),
                checkout,
                label: holder.checkout_label(),
            });
        }
        Some(TabBoundary {
            effective_dir: row.path.clone(),
            project_dir: project_dir.to_string_lossy().into_owned(),
            source: "project".to_string(),
            repository: info.project_name(),
            is_git_repo: true,
            is_worktree: info.is_worktree,
            branch: info.branch.clone(),
            base_ref: info.base_ref.clone(),
            uncommitted: info.uncommitted,
            ahead: info.ahead,
            changed_files: info.changed_files,
            insertions: info.insertions,
            deletions: info.deletions,
            ..Default::default()
        })
    }

    fn override_branch(
        &self,
        projects: &mut [vmux_api::space::ProjectRow],
        boundary: &mut Option<TabBoundary>,
    ) {
        let Some(tab) = self.focus.tab else {
            return;
        };
        if let Ok(worktree) = self.worktrees.get(tab) {
            if let Some(project) = projects.iter_mut().find(|project| project.is_active) {
                project.branch.clone_from(&worktree.branch);
            }
            if let Some(boundary) = boundary.as_mut() {
                boundary.branch.clone_from(&worktree.branch);
            }
        }
        if let Ok(pending) = self.pending.get(tab)
            && !pending.branch().is_empty()
            && boundary
                .as_ref()
                .is_some_and(|boundary| pending.matches_project_dir(&boundary.project_dir))
        {
            if let Some(project) = projects.iter_mut().find(|project| project.is_active) {
                project.branch = pending.branch().to_string();
            }
            if let Some(boundary) = boundary.as_mut() {
                boundary.branch = pending.branch().to_string();
            }
        }
    }
}

fn push_projects_host_emit(
    mut projection: LayoutProjection<TabBoundaryState>,
    mut rows: ProjectRows,
    selections: Query<(Ref<LayoutCef>, &ProjectSelectionReceipt)>,
) {
    let selection = rows.selection();
    let Some((layout, _)) = projection.layout.get() else {
        return;
    };
    let changed = match selections.get(layout) {
        Ok((owner, receipt)) => owner.is_added() || receipt.0 != selection,
        Err(_) => true,
    };
    let Some(target) = projection.target_for(changed) else {
        return;
    };
    let payload = rows.snapshot(target.entity);
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    projection
        .commands
        .entity(target.entity)
        .insert(ProjectSelectionReceipt(selection));
    if !projection.should_emit(target, body) {
        return;
    }
    projection
        .commands
        .entity(target.entity)
        .insert(ProjectProjection(payload.clone()));
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(
            target.entity,
            &payload,
        ));
}

#[derive(bevy::ecs::system::SystemParam)]
struct BookmarkRows<'w, 's> {
    pins: Query<
        'w,
        's,
        (
            &'static Uuid,
            &'static PageMetadata,
            &'static BookmarkOrder,
            Has<Bookmark>,
        ),
        With<Pin>,
    >,
    folders: Query<
        'w,
        's,
        (
            Entity,
            &'static Uuid,
            &'static Name,
            Option<&'static Children>,
            Has<Collapsed>,
            Option<&'static SmartBookmarkFolder>,
            &'static BookmarkOrder,
            Option<&'static ChildOf>,
        ),
        With<Folder>,
    >,
    roots: Query<
        'w,
        's,
        (
            &'static Uuid,
            &'static PageMetadata,
            &'static BookmarkOrder,
            Has<Pin>,
        ),
        (With<Bookmark>, Without<ChildOf>),
    >,
    bookmarks: Query<
        'w,
        's,
        (
            &'static Uuid,
            &'static PageMetadata,
            &'static BookmarkOrder,
            Has<Pin>,
        ),
        With<Bookmark>,
    >,
}

impl BookmarkRows<'_, '_> {
    fn row(uuid: &Uuid, metadata: &PageMetadata, bookmarked: bool, pinned: bool) -> BookmarkRow {
        BookmarkRow {
            uuid: uuid.0.clone(),
            metadata: metadata.clone(),
            bookmarked,
            pinned,
        }
    }

    fn snapshot(&self) -> BookmarkStateEvent {
        let mut pins = Vec::new();
        for (uuid, metadata, order, bookmarked) in &self.pins {
            pins.push((order.0, Self::row(uuid, metadata, bookmarked, true)));
        }
        pins.sort_by_key(|(order, _)| *order);
        let mut pin_rows = Vec::new();
        for (_, row) in pins {
            pin_rows.push(row);
        }
        let mut roots = Vec::new();
        for (_, uuid, name, children, collapsed, smart, order, parent) in &self.folders {
            if smart.is_some() {
                continue;
            }
            let mut entries = Vec::new();
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok((uuid, metadata, order, pinned)) = self.bookmarks.get(child) {
                        entries.push((order.0, Self::row(uuid, metadata, true, pinned)));
                    }
                }
            }
            entries.sort_by_key(|(order, _)| *order);
            let mut kids = Vec::new();
            for (_, row) in entries {
                kids.push(row);
            }
            let parent = parent.and_then(|parent| {
                self.folders
                    .get(parent.get())
                    .ok()
                    .map(|(_, uuid, _, _, _, _, _, _)| uuid.0.clone())
            });
            roots.push((
                order.0,
                BookmarkNode::Folder(BookmarkFolderRow {
                    uuid: uuid.0.clone(),
                    name: name.as_str().to_string(),
                    collapsed,
                    parent,
                    children: kids,
                }),
            ));
        }
        for (uuid, metadata, order, pinned) in &self.roots {
            roots.push((
                order.0,
                BookmarkNode::Entry(Self::row(uuid, metadata, true, pinned)),
            ));
        }
        roots.sort_by_key(|(order, _)| *order);
        let mut nodes = Vec::new();
        for (_, node) in roots {
            nodes.push(node);
        }
        let folders = BookmarkFolders::from_nodes(&nodes).0;
        BookmarkStateEvent {
            pins: pin_rows,
            roots: nodes,
            folders,
        }
    }
}

fn push_bookmarks_host_emit(
    mut projection: LayoutProjection<BookmarkStateEvent>,
    rows: BookmarkRows,
) {
    let Some(target) = projection.target() else {
        return;
    };
    let payload = rows.snapshot();
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, body) {
        return;
    }
    projection
        .commands
        .entity(target.entity)
        .insert(BookmarkProjection(payload.clone()));
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(
            target.entity,
            &payload,
        ));
}

fn push_tabs_host_emit(mut projection: LayoutProjection<TabListState>, tabs: TabProjectionData) {
    let Some(target) = projection.target() else {
        return;
    };
    let cef_e = target.entity;

    let payload = TabListState { tabs: tabs.rows() };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, body) {
        return;
    }
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &payload));
}

#[derive(Component)]
struct UpdateNoticeReceipt(UpdateState);

#[derive(bevy::ecs::system::SystemParam)]
struct LayoutDelivery<'w, 's> {
    browsers: NonSend<'w, Browsers>,
    layouts: Query<'w, 's, (), With<LayoutCef>>,
}

impl LayoutDelivery<'_, '_> {
    fn available(&self, entity: Entity) -> bool {
        self.layouts.contains(entity) && self.browsers.can_emit_to(&entity)
    }
}

fn retire_update_notice(removed: On<Remove, LayoutCef>, mut commands: Commands) {
    commands
        .entity(removed.event_target())
        .try_remove::<UpdateNoticeReceipt>();
}

fn push_update_notice_emit(
    mut commands: Commands,
    delivery: LayoutDelivery,
    layout: FocusedLayout,
    state: Single<&UpdateState>,
    receipts: Query<&UpdateNoticeReceipt, With<LayoutCef>>,
) {
    let Some((cef_e, page_ready_changed)) = layout.get() else {
        return;
    };
    if !delivery.available(cef_e) {
        return;
    }
    let unchanged = receipts
        .get(cef_e)
        .is_ok_and(|previous| previous.0 == **state);
    if unchanged && (!page_ready_changed || **state == UpdateState::Idle) {
        return;
    }
    match &*state {
        UpdateState::Idle => {
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                cef_e,
                &UpdateCleared,
            ));
        }
        UpdateState::Downloading {
            version,
            downloaded,
            total,
        } => {
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                cef_e,
                &UpdateProgress {
                    version: version.clone(),
                    downloaded: *downloaded,
                    total: *total,
                    installing: false,
                },
            ));
        }
        UpdateState::Installing { version } => {
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                cef_e,
                &UpdateProgress {
                    version: version.clone(),
                    downloaded: 0,
                    total: 0,
                    installing: true,
                },
            ));
        }
        UpdateState::Ready { version } => {
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                cef_e,
                &UpdateReady {
                    version: version.clone(),
                },
            ));
        }
    }
    commands
        .entity(cef_e)
        .insert(UpdateNoticeReceipt(state.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_api::UiStateProjection;
    use vmux_layout::state::UpdateStatus;

    #[derive(Resource, Default)]
    struct ProjectionWrites(Vec<(Entity, vmux_layout::state::LayoutUiStatePatch)>);

    impl ProjectionWrites {
        fn families(&self, entity: Entity) -> Vec<&'static str> {
            let mut families = std::collections::BTreeSet::new();
            for (target, patch) in &self.0 {
                if *target != entity {
                    continue;
                }
                for (name, present) in [
                    ("bookmarks", patch.bookmarks.is_some()),
                    ("geometry", patch.layout.is_some()),
                    ("pane_tree", patch.pane_tree.is_some()),
                    ("projects", patch.projects.is_some()),
                    ("stacks", patch.stacks.is_some()),
                    ("tabs", patch.tabs.is_some()),
                ] {
                    if present {
                        families.insert(name);
                    }
                }
            }
            families.into_iter().collect()
        }
    }

    fn record_projections(
        write: On<UiStateWrite<LayoutUiState>>,
        mut writes: ResMut<ProjectionWrites>,
    ) {
        let patch = write.update();
        if patch.bookmarks.is_some()
            || patch.layout.is_some()
            || patch.pane_tree.is_some()
            || patch.projects.is_some()
            || patch.stacks.is_some()
            || patch.tabs.is_some()
        {
            writes.0.push((write.webview(), patch.clone()));
        }
    }

    struct ProjectionPage {
        window: Entity,
        layout: Entity,
        tab: Entity,
        pane: Entity,
        stack: Entity,
    }

    struct ProjectionPages {
        app: App,
        focus: Entity,
    }

    impl ProjectionPages {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatePlugin))
                .insert_resource(AppSettings::default())
                .init_resource::<vmux_space::ActiveProjectRows>()
                .init_non_send::<Browsers>()
                .init_resource::<ProjectionWrites>()
                .add_observer(record_projections);
            app.world_mut().spawn(UpdateState::Idle);
            let focus = app
                .world_mut()
                .spawn(vmux_layout::active_pane::ActiveStack::default().local_bundle())
                .id();
            Self { app, focus }
        }

        fn page(&mut self, title: &str) -> ProjectionPage {
            let window = self.app.world_mut().spawn(Window::default()).id();
            let root = self.app.world_mut().spawn(HostWindow(window)).id();
            let layout = self
                .app
                .world_mut()
                .spawn((LayoutCef, PageReady {}, ChildOf(root)))
                .id();
            let tab = self
                .app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(1), ChildOf(root)))
                .id();
            let pane = self
                .app
                .world_mut()
                .spawn((Pane, Active, LastActivatedAt(1), ChildOf(tab)))
                .id();
            let stack = self
                .app
                .world_mut()
                .spawn((
                    Stack::default(),
                    Active,
                    LastActivatedAt(1),
                    ChildOf(pane),
                    PageMetadata {
                        title: title.into(),
                        url: "vmux://start/".into(),
                        ..Default::default()
                    },
                ))
                .id();
            self.app
                .world_mut()
                .non_send_mut::<Browsers>()
                .set_externally_hosted(layout);
            ProjectionPage {
                window,
                layout,
                tab,
                pane,
                stack,
            }
        }

        fn focus(&mut self, page: &ProjectionPage) {
            let world = self.app.world_mut();
            let mut windows = world.query_filtered::<Entity, (With<Window>, With<Active>)>();
            let active: Vec<_> = windows.iter(world).collect();
            for entity in active {
                world.entity_mut(entity).remove::<Active>();
            }
            world.entity_mut(page.window).insert(Active);
            let mut tabs = world.query_filtered::<Entity, (With<Tab>, With<Active>)>();
            let active_tabs: Vec<_> = tabs.iter(world).collect();
            for entity in active_tabs {
                world.entity_mut(entity).remove::<Active>();
            }
            world.entity_mut(page.tab).insert(Active);
            world
                .entity_mut(self.focus)
                .insert(vmux_layout::active_pane::ActiveStack {
                    tab: Some(page.tab),
                    pane: Some(page.pane),
                    stack: Some(page.stack),
                });
        }

        fn update(&mut self) -> ProjectionWrites {
            self.app.update();
            ProjectionWrites(std::mem::take(
                &mut self.app.world_mut().resource_mut::<ProjectionWrites>().0,
            ))
        }

        fn available(&mut self, page: &ProjectionPage, available: bool) {
            let mut browsers = self.app.world_mut().non_send_mut::<Browsers>();
            if available {
                browsers.set_externally_hosted(page.layout);
            } else {
                browsers.unset_externally_hosted(page.layout);
            }
        }
    }

    #[test]
    fn projection_receipts_keep_all_payload_families_independent() {
        let mut pages = ProjectionPages::new();
        let page = pages.page("first");
        pages.focus(&page);
        let writes = pages.update();
        assert_eq!(
            writes.families(page.layout),
            [
                "bookmarks",
                "geometry",
                "pane_tree",
                "projects",
                "stacks",
                "tabs"
            ]
        );
        assert!(pages.update().0.is_empty());
        pages
            .app
            .world_mut()
            .entity_mut(page.layout)
            .insert(PageReady {});
        assert_eq!(
            pages.update().families(page.layout),
            writes.families(page.layout)
        );
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_retire_before_a_readded_layout_becomes_available() {
        let mut pages = ProjectionPages::new();
        let page = pages.page("first");
        pages.focus(&page);
        let families = pages.update().families(page.layout);
        assert_eq!(families.len(), 6);
        pages.available(&page, false);
        pages
            .app
            .world_mut()
            .entity_mut(page.layout)
            .remove::<LayoutCef>();
        pages
            .app
            .world_mut()
            .entity_mut(page.layout)
            .insert(LayoutCef);
        assert!(pages.update().0.is_empty());
        assert!(pages.update().0.is_empty());
        pages.available(&page, true);
        assert_eq!(pages.update().families(page.layout), families);
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_preserve_each_window_when_focus_returns() {
        let mut pages = ProjectionPages::new();
        let first = pages.page("first");
        let second = pages.page("second");
        pages.focus(&first);
        assert_eq!(pages.update().families(first.layout).len(), 6);
        pages.focus(&second);
        assert_eq!(pages.update().families(second.layout).len(), 6);
        pages.focus(&first);
        assert!(pages.update().0.is_empty());
        pages
            .app
            .world_mut()
            .get_mut::<PageMetadata>(first.stack)
            .unwrap()
            .title = "renamed".into();
        let changed = pages.update();
        assert!(changed.0.iter().all(|(entity, _)| *entity == first.layout));
        let navigation = changed
            .0
            .iter()
            .find_map(|(_, patch)| patch.stacks.as_ref())
            .unwrap();
        assert_eq!(navigation.stacks[0].title, "renamed");
        let panes = changed
            .0
            .iter()
            .find_map(|(_, patch)| patch.pane_tree.as_ref())
            .unwrap();
        assert_eq!(panes.panes[0].stacks[0].title, "renamed");
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_do_not_acknowledge_an_unavailable_layout() {
        let mut pages = ProjectionPages::new();
        let page = pages.page("first");
        pages.focus(&page);
        pages.available(&page, false);
        assert!(pages.update().0.is_empty());
        pages.app.world_mut().entity_mut(page.tab).insert(
            vmux_layout::page_context::PendingWorkspaceSelection::new(
                "/project".into(),
                "feature".into(),
                1,
            ),
        );
        assert!(pages.update().0.is_empty());
        pages.available(&page, true);
        assert_eq!(pages.update().families(page.layout).len(), 6);
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_update_geometry_after_resize_without_focus_churn() {
        let mut pages = ProjectionPages::new();
        let page = pages.page("first");
        pages.focus(&page);
        pages.app.world_mut().spawn((
            Header,
            Open,
            ChildOf(page.layout),
            ComputedNode::from_origin(Vec2::new(100.0, 50.0)),
        ));
        let initial = pages.update();
        let right = initial
            .0
            .iter()
            .find_map(|(_, patch)| patch.layout.as_ref())
            .unwrap()
            .header_right
            .unwrap();
        assert!(pages.update().0.is_empty());
        pages
            .app
            .world_mut()
            .get_mut::<Window>(page.window)
            .unwrap()
            .focused = false;
        assert!(pages.update().0.is_empty());
        let width = pages
            .app
            .world()
            .get::<Window>(page.window)
            .unwrap()
            .resolution
            .width();
        pages
            .app
            .world_mut()
            .get_mut::<Window>(page.window)
            .unwrap()
            .resolution
            .set(width + 200.0, 900.0);
        let resized = pages.update();
        assert_eq!(resized.families(page.layout), ["geometry"]);
        let updated = resized
            .0
            .iter()
            .find_map(|(_, patch)| patch.layout.as_ref())
            .unwrap()
            .header_right
            .unwrap();
        assert_eq!(updated - right, 200.0);
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_refuse_a_surface_reparented_out_of_its_stack() {
        let mut pages = ProjectionPages::new();
        let first = pages.page("first");
        let second = pages.page("second");
        pages.focus(&first);
        pages
            .app
            .world_mut()
            .get_mut::<PageMetadata>(first.stack)
            .unwrap()
            .url
            .clear();
        let browser = pages
            .app
            .world_mut()
            .spawn((
                Browser,
                ChildOf(first.stack),
                PageMetadata {
                    title: "hosted page".into(),
                    url: "https://example.test/".into(),
                    ..Default::default()
                },
            ))
            .id();
        pages
            .app
            .world_mut()
            .entity_mut(first.stack)
            .insert(vmux_layout::page_slot::Current::new(
                Some(browser),
                PageMetadata {
                    title: "hosted page".into(),
                    url: "https://example.test/".into(),
                    ..Default::default()
                },
            ));
        let initial = pages.update();
        let pane = initial
            .0
            .iter()
            .find_map(|(_, patch)| patch.pane_tree.as_ref())
            .unwrap();
        assert_eq!(pane.panes[0].stacks[0].url, "https://example.test/");
        let initial_tabs = initial
            .0
            .iter()
            .find_map(|(_, patch)| patch.tabs.as_ref())
            .unwrap();
        assert_eq!(initial_tabs.tabs[0].url, "https://example.test/");
        pages
            .app
            .world_mut()
            .entity_mut(browser)
            .insert(ChildOf(second.stack));
        let moved = pages.update();
        let pane = moved
            .0
            .iter()
            .find_map(|(_, patch)| patch.pane_tree.as_ref())
            .unwrap();
        assert!(pane.panes[0].stacks[0].url.is_empty());
        let tabs = moved
            .0
            .iter()
            .find_map(|(_, patch)| patch.tabs.as_ref())
            .unwrap();
        assert!(tabs.tabs[0].url.is_empty());
        assert!(pages.update().0.is_empty());
    }

    #[derive(Resource, Default)]
    struct PublishedUpdates(Vec<(Entity, LayoutUiState)>);

    fn record_updates(
        write: On<UiStateWrite<LayoutUiState>>,
        mut updates: ResMut<PublishedUpdates>,
    ) {
        let patch = write.update();
        if patch.update_progress.is_none()
            && patch.update_ready.is_none()
            && patch.update_cleared.is_none()
        {
            return;
        }
        let mut state = LayoutUiState::default();
        state.apply(patch.clone());
        updates.0.push((write.webview(), state));
    }

    struct UpdatePages {
        app: App,
        state: Entity,
    }

    impl UpdatePages {
        fn new(state: UpdateState) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatePlugin))
                .insert_resource(AppSettings::default())
                .init_resource::<vmux_space::ActiveProjectRows>()
                .init_non_send::<Browsers>()
                .init_resource::<PublishedUpdates>()
                .add_observer(record_updates);
            let state = app.world_mut().spawn(state).id();
            Self { app, state }
        }

        fn page(&mut self) -> Entity {
            let page = self.app.world_mut().spawn((LayoutCef, PageReady {})).id();
            self.app
                .world_mut()
                .non_send_mut::<Browsers>()
                .set_externally_hosted(page);
            page
        }

        fn update(&mut self) -> Vec<(Entity, Option<UpdateStatus>)> {
            self.app.update();
            let mut results = Vec::new();
            for (entity, state) in self
                .app
                .world_mut()
                .resource_mut::<PublishedUpdates>()
                .0
                .drain(..)
            {
                results.push((entity, state.update));
            }
            results
        }

        fn set(&mut self, state: UpdateState) {
            self.app.world_mut().entity_mut(self.state).insert(state);
        }
    }

    #[test]
    fn update_notice_replays_on_readiness_and_clears_when_update_finishes() {
        let mut pages = UpdatePages::new(UpdateState::Ready {
            version: "v2".into(),
        });
        let page = pages.page();
        let ready = vec![(
            page,
            Some(UpdateStatus::Ready {
                version: "v2".into(),
            }),
        )];
        assert_eq!(pages.update(), ready);
        assert!(pages.update().is_empty());
        pages.app.world_mut().entity_mut(page).insert(PageReady {});
        assert_eq!(pages.update(), ready);
        pages.set(UpdateState::Idle);
        assert_eq!(pages.update(), vec![(page, None)]);
        pages.app.world_mut().entity_mut(page).insert(PageReady {});
        assert!(pages.update().is_empty());
    }

    #[test]
    fn update_notice_restarts_after_layout_owner_is_reattached() {
        let mut pages = UpdatePages::new(UpdateState::Idle);
        let page = pages.page();
        assert_eq!(pages.update(), vec![(page, None)]);
        assert!(pages.update().is_empty());
        pages.app.world_mut().entity_mut(page).remove::<LayoutCef>();
        pages.app.world_mut().entity_mut(page).insert(LayoutCef);
        assert_eq!(pages.update(), vec![(page, None)]);
        pages.app.world_mut().entity_mut(page).remove::<LayoutCef>();
        assert!(pages.update().is_empty());
        assert!(pages.app.world().get::<UpdateNoticeReceipt>(page).is_none());
    }

    #[test]
    fn update_notice_publications_stay_independent_when_window_focus_changes() {
        let mut pages = UpdatePages::new(UpdateState::Ready {
            version: "v2".into(),
        });
        let first_window = pages
            .app
            .world_mut()
            .spawn((Window::default(), Active))
            .id();
        let second_window = pages.app.world_mut().spawn(Window::default()).id();
        let first = pages.page();
        let second = pages.page();
        pages
            .app
            .world_mut()
            .entity_mut(first)
            .insert(HostWindow(first_window));
        pages
            .app
            .world_mut()
            .entity_mut(second)
            .insert(HostWindow(second_window));
        assert_eq!(
            pages.update(),
            vec![(
                first,
                Some(UpdateStatus::Ready {
                    version: "v2".into()
                })
            )]
        );
        pages
            .app
            .world_mut()
            .entity_mut(first_window)
            .remove::<Active>();
        pages
            .app
            .world_mut()
            .entity_mut(second_window)
            .insert(Active);
        assert_eq!(
            pages.update(),
            vec![(
                second,
                Some(UpdateStatus::Ready {
                    version: "v2".into()
                })
            )]
        );
        pages
            .app
            .world_mut()
            .entity_mut(second_window)
            .remove::<Active>();
        pages
            .app
            .world_mut()
            .entity_mut(first_window)
            .insert(Active);
        assert!(pages.update().is_empty());
    }

    #[test]
    fn update_notice_does_not_acknowledge_an_unavailable_browser() {
        let mut pages = UpdatePages::new(UpdateState::Downloading {
            version: "v2".into(),
            downloaded: 3,
            total: 10,
        });
        let page = pages.app.world_mut().spawn((LayoutCef, PageReady {})).id();
        assert!(pages.update().is_empty());
        pages
            .app
            .world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(page);
        assert_eq!(
            pages.update(),
            vec![(
                page,
                Some(UpdateStatus::Downloading {
                    version: "v2".into(),
                    downloaded: 3,
                    total: 10
                })
            )]
        );
        assert!(pages.update().is_empty());
        pages.set(UpdateState::Installing {
            version: "v2".into(),
        });
        assert_eq!(
            pages.update(),
            vec![(
                page,
                Some(UpdateStatus::Installing {
                    version: "v2".into()
                })
            )]
        );
    }

    #[test]
    fn dormant_stack_keeps_its_favicon_without_a_loading_indicator() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatePlugin))
            .insert_resource(AppSettings::default())
            .init_resource::<vmux_space::ActiveProjectRows>()
            .init_non_send::<Browsers>();
        app.world_mut().spawn(UpdateState::Idle);
        let layout = app.world_mut().spawn((LayoutCef, PageReady {})).id();
        app.world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(layout);
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(tab)))
            .id();
        let metadata = PageMetadata {
            title: "Start".into(),
            url: "vmux://start/".into(),
            icon: PageIcon::Builtin(vmux_api::BuiltinIcon::Sparkles),
            ..Default::default()
        };
        let dormant = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt(1),
                metadata.clone(),
                ChildOf(pane),
            ))
            .id();
        let selected = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt(2),
                metadata.clone(),
                Active,
                ChildOf(pane),
            ))
            .id();
        app.world_mut().spawn(
            vmux_layout::active_pane::ActiveStack {
                tab: Some(tab),
                pane: Some(pane),
                stack: Some(selected),
            }
            .local_bundle(),
        );
        app.update();
        let projection = &app.world().get::<PaneTreeProjection>(layout).unwrap().0;
        let rows = &projection.panes[0].stacks;
        let start = rows.iter().find(|row| row.id == dormant.to_bits()).unwrap();
        assert!(!start.is_loading);
        assert_eq!(start.icon, metadata.icon);
        assert!(
            rows.iter()
                .find(|row| row.id == selected.to_bits())
                .unwrap()
                .is_loading
        );

        let browser = app
            .world_mut()
            .spawn((Browser, Loading, metadata.clone(), ChildOf(dormant)))
            .id();
        app.world_mut()
            .entity_mut(dormant)
            .insert(vmux_layout::page_slot::Current::new(
                Some(browser),
                metadata,
            ));
        app.update();
        let projection = &app.world().get::<PaneTreeProjection>(layout).unwrap().0;
        assert!(
            projection.panes[0]
                .stacks
                .iter()
                .find(|row| row.id == dormant.to_bits())
                .unwrap()
                .is_loading
        );
    }

    #[test]
    fn retiring_tabs_and_stacks_leave_navigation_immediately() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let space = app.world_mut().spawn_empty().id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(2), ChildOf(space)))
            .id();
        let retiring_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(3), Retiring, ChildOf(space)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(pane)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            LastActivatedAt(3),
            Retiring,
            ChildOf(pane),
        ));

        let (active_tab, active_stack, rows) = app
            .world_mut()
            .run_system_once(move |projection: TabProjectionData| {
                (
                    projection.active_tab.get(),
                    projection.active_stack(tab),
                    projection.rows(),
                )
            })
            .unwrap();

        assert_eq!(active_tab, Some(tab));
        assert_eq!(active_stack, Some(stack));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, tab.to_bits().to_string());
        assert!(rows[0].is_active);
        assert_ne!(rows[0].id, retiring_tab.to_bits().to_string());
    }

    #[test]
    fn retiring_state_invalidates_layout_projection() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatePlugin))
            .insert_resource(AppSettings::default())
            .init_resource::<vmux_space::ActiveProjectRows>();
        app.world_mut().insert_non_send(Browsers::default());
        app.world_mut().spawn(UpdateState::Idle);
        app.update();
        let revision = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        let closing = app.world_mut().spawn(Retiring).id();

        app.update();

        let added = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        assert!(added > revision);

        app.world_mut().entity_mut(closing).remove::<Retiring>();
        app.update();

        let removed = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        assert!(removed > added);
    }

    #[test]
    fn computed_header_resize_invalidates_layout_geometry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, mark_computed_layout_dirty);
        app.world_mut().spawn(LayoutGeometryRevision::default());
        let header = app
            .world_mut()
            .spawn((Header, ComputedNode::default()))
            .id();

        app.update();
        let added = app
            .world_mut()
            .query::<&LayoutGeometryRevision>()
            .single(app.world())
            .unwrap()
            .0;
        app.update();
        let stable = app
            .world_mut()
            .query::<&LayoutGeometryRevision>()
            .single(app.world())
            .unwrap()
            .0;
        assert_eq!(stable, added);

        app.world_mut()
            .get_mut::<ComputedNode>(header)
            .unwrap()
            .size = Vec2::new(800.0, 84.0);
        app.update();

        let resized = app
            .world_mut()
            .query::<&LayoutGeometryRevision>()
            .single(app.world())
            .unwrap()
            .0;
        assert!(resized > stable);
    }

    #[test]
    fn pending_stack_metadata_wins_before_the_surface_commits() {
        let presented = PageMetadata {
            url: "git://old".to_string(),
            title: "Old".to_string(),
            ..Default::default()
        };
        let pending = PageMetadata {
            url: "vmux://projects/".to_string(),
            title: "Projects".to_string(),
            ..Default::default()
        };
        let identity = PageIdentity::from("Old page identity");

        let projected = PagePresentation::metadata(Some(&pending), Some(&presented)).unwrap();

        assert_eq!(projected.url, "vmux://projects/");
        assert!(PagePresentation::identity(projected, Some(&presented), Some(&identity)).is_none());
    }

    #[test]
    fn window_focus_does_not_dirty_layout_geometry() {
        let mut window = Window::default();
        let before = WindowGeometryState::of(&window);
        window.focused = !window.focused;

        assert_eq!(WindowGeometryState::of(&window), before);
    }

    #[test]
    fn pending_workspace_selection_does_not_invalidate_unrelated_projections() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatePlugin))
            .insert_resource(AppSettings::default())
            .init_resource::<vmux_space::ActiveProjectRows>();
        app.world_mut().insert_non_send(Browsers::default());
        app.world_mut().spawn(UpdateState::Idle);
        app.update();
        let revision = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        let tab = app
            .world_mut()
            .spawn(vmux_layout::page_context::PendingWorkspaceSelection::new(
                "/project".into(),
                "feature".into(),
                1,
            ))
            .id();

        app.update();

        let after_add = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        assert_eq!(after_add, revision);

        app.world_mut()
            .entity_mut(tab)
            .remove::<vmux_layout::page_context::PendingWorkspaceSelection>();
        app.update();

        let after_remove = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        assert_eq!(after_remove, after_add);
    }
}
