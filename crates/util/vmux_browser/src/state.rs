use bevy::{ecs::entity::EntityHashMap, ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use vmux_api::bookmark::{
    BookmarkEntry, BookmarkFolderChoice, BookmarkFolderRow, BookmarkNode, BookmarkRow,
    BookmarkStateEvent, SmartBookmarkFolder,
};
use vmux_api::{Id, PageIcon, VmuxRoute};
use vmux_ecs::{
    Active, Bookmark, BookmarkOrder, Collapsed, Folder, PageIdentity, PageMetadata, Pin,
    UiStateWrite,
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
    state::UiLayout,
    tab::Tab,
    toggle::LayoutHidden,
    window::{FocusedWindow, VmuxWindow, WindowHierarchy},
};

use vmux_setting::AppSettings;
use vmux_space::{ExpandedProjectDirs, SpaceProjects};

use crate::host::{LayoutFixedOffsets, LayoutWindowPadding, UpdateProjection};
use vmux_flex::prelude::*;

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
    tabs: Query<'w, 's, (Entity, &'static Name, &'static LastActivatedAt), (With<Tab>, Without<Retiring>)>,
    hierarchy: vmux_layout::tab::TabHierarchy<'w, 's>,
    active_tab: ActiveTabParam<'w, 's>,
    focus: LayoutFocus<'w, 's>,
    stack_timestamps:
        Query<'w, 's, (Entity, &'static LastActivatedAt), (With<Stack>, Without<Retiring>)>,
    surfaces: StackSurface<'w, 's>,
    stack_metadata: Query<'w, 's, &'static PageMetadata, With<Stack>>,
    browser_metadata:
        Query<'w, 's, (&'static PageMetadata, Option<&'static PageIdentity>), With<Browser>>,
    done_agents: Query<'w, 's, Entity, With<AgentDoneUnseen>>,
}

impl TabProjectionData<'_, '_> {
    fn active_stack(&self, tab: Entity) -> Option<Entity> {
        self.focus
            .leaves(tab)
            .into_iter()
            .filter_map(|pane| self.focus.stack(pane))
            .filter_map(|stack| self.stack_timestamps.get(stack).ok())
            .max_by_key(|(_, timestamp)| timestamp.milliseconds())
            .map(|(entity, _)| entity)
    }

    fn page(&self, stack: Entity) -> Option<(PageMetadata, Option<&PageIdentity>)> {
        let presented = self
            .surfaces
            .current(stack)
            .and_then(|surface| self.browser_metadata.get(surface).ok());
        let pending = self.stack_metadata.get(stack).ok();
        let metadata =
            PagePresentation::metadata(pending, presented.map(|(metadata, _)| metadata))?;
        let identity = PagePresentation::identity(
            metadata,
            presented.map(|(presented, _)| presented),
            presented.and_then(|(_, identity)| identity),
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
            let Ok((entity, name, _)) = self.tabs.get(entity) else {
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
                name: if name.as_str().is_empty() {
                    "Tab".to_string()
                } else {
                    name.as_str().to_string()
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

pub(crate) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_revision)
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
                    .after(LayoutSystems::Layout),
            );
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

#[derive(Default)]
struct ProjectionCache {
    revisions: EntityHashMap<u64>,
    bodies: EntityHashMap<String>,
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

impl ProjectionCache {
    fn needs_rebuild(&self, entity: Entity, revision: u64, page_ready_changed: bool) -> bool {
        page_ready_changed || self.revisions.get(&entity) != Some(&revision)
    }

    fn should_emit(
        &mut self,
        entity: Entity,
        revision: u64,
        body: String,
        page_ready_changed: bool,
    ) -> bool {
        self.revisions.insert(entity, revision);
        let previous = self
            .bodies
            .get(&entity)
            .map(String::as_str)
            .unwrap_or_default();
        if !page_ready_changed && body == previous {
            return false;
        }
        self.bodies.insert(entity, body);
        true
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct LayoutProjection<'w, 's> {
    commands: Commands<'w, 's>,
    browsers: NonSend<'w, Browsers>,
    layout: FocusedLayout<'w, 's>,
    revision: Single<'w, 's, &'static StateRevision>,
    cache: Local<'s, ProjectionCache>,
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

#[derive(Default)]
struct ProjectProjectionCursor(EntityHashMap<Option<ProjectProjectionSelection>>);

impl ProjectProjectionCursor {
    fn needs_emit(&self, entity: Entity, selection: &Option<ProjectProjectionSelection>) -> bool {
        self.0.get(&entity) != Some(selection)
    }

    fn record(&mut self, entity: Entity, selection: Option<ProjectProjectionSelection>) {
        self.0.insert(entity, selection);
    }
}

impl LayoutProjection<'_, '_> {
    fn target(&self) -> Option<ProjectionTarget> {
        self.target_for_revision(self.revision.0, false)
    }

    fn target_for(&self, force: bool) -> Option<ProjectionTarget> {
        self.target_for_revision(self.revision.0, force)
    }

    fn target_for_revision(&self, revision: u64, force: bool) -> Option<ProjectionTarget> {
        let (entity, page_ready_changed) = self.layout.get()?;
        if !self.browsers.can_emit_to(&entity) {
            return None;
        }
        let target = ProjectionTarget {
            entity,
            revision,
            page_ready_changed,
        };
        (force
            || self
                .cache
                .needs_rebuild(entity, target.revision, page_ready_changed))
        .then_some(target)
    }

    fn should_emit(&mut self, target: ProjectionTarget, body: String) -> bool {
        self.cache.should_emit(
            target.entity,
            target.revision,
            body,
            target.page_ready_changed,
        )
    }
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

#[derive(Default)]
struct WindowGeometryCache(EntityHashMap<WindowGeometryState>);

impl WindowGeometryCache {
    fn update<'a>(
        &mut self,
        windows: impl Iterator<Item = (Entity, &'a Window)>,
        removed: impl Iterator<Item = Entity>,
    ) -> bool {
        let mut changed = false;
        for (entity, window) in windows {
            let geometry = WindowGeometryState::of(window);
            changed |= self.0.insert(entity, geometry) != Some(geometry);
        }
        for entity in removed {
            changed |= self.0.remove(&entity).is_some();
        }
        changed
    }
}

#[derive(Default)]
struct FocusedWindowCursor(Option<Entity>);

impl FocusedWindowCursor {
    fn update(&mut self, current: Option<Entity>) -> bool {
        if self.0 == current {
            return false;
        }
        self.0 = current;
        true
    }
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
    Changed<LayoutHidden>,
    Changed<Retiring>,
)>;

type BookmarkProjectionChanged = Or<(
    Changed<Id<BookmarkEntry>>,
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
    uuid: RemovedComponents<'w, 's, Id<BookmarkEntry>>,
    bookmark_order: RemovedComponents<'w, 's, BookmarkOrder>,
    bookmark: RemovedComponents<'w, 's, Bookmark>,
    pin: RemovedComponents<'w, 's, Pin>,
    folder: RemovedComponents<'w, 's, Folder>,
    name: RemovedComponents<'w, 's, Name>,
    collapsed: RemovedComponents<'w, 's, Collapsed>,
    smart_folder: RemovedComponents<'w, 's, SmartBookmarkFolder>,
    expanded_projects: RemovedComponents<'w, 's, ExpandedProjectDirs>,
    layout_hidden: RemovedComponents<'w, 's, LayoutHidden>,
    retiring: RemovedComponents<'w, 's, Retiring>,
}

impl PageStateRemovals<'_, '_> {
    fn changes(&mut self) -> PageStateRemovalChanges {
        let header = self.header.read().count() > 0;
        let open = self.open.read().count() > 0;
        let side_sheet = self.side_sheet.read().count() > 0;
        let side_sheet_position = self.side_sheet_position.read().count() > 0;
        let vmux_window = self.vmux_window.read().count() > 0;
        let removed_windows = self.window.read().collect::<Vec<_>>();
        let window = !removed_windows.is_empty();
        let node = self.node.read().count() > 0;
        let host_window = self.host_window.read().count() > 0;
        let layout_hidden = self.layout_hidden.read().count() > 0;
        let geometry = header
            || open
            || side_sheet
            || side_sheet_position
            || vmux_window
            || window
            || node
            || host_window
            || layout_hidden;
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
        PageStateRemovalChanges {
            state,
            geometry,
            removed_windows,
        }
    }
}

struct PageStateRemovalChanges {
    state: bool,
    geometry: bool,
    removed_windows: Vec<Entity>,
}

fn mark_page_dirty(
    changes: StateChanges,
    geometry: LayoutGeometryChanges,
    mut removals: PageStateRemovals,
    focused_window: FocusedWindow,
    focused_stack: FocusedStack,
    settings: Res<AppSettings>,
    repo_info: Option<Single<Ref<RepoInfoCache>>>,
    windows: Query<(Entity, &Window), Changed<Window>>,
    mut window_geometry: Local<WindowGeometryCache>,
    mut focused_window_cursor: Local<FocusedWindowCursor>,
    mut revision: Single<&mut StateRevision>,
    mut geometry_revision: Single<&mut LayoutGeometryRevision>,
) {
    let removals = removals.changes();
    let focused_window_changed = focused_window_cursor.update(focused_window.entity());
    let window_geometry_changed =
        window_geometry.update(windows.iter(), removals.removed_windows.iter().copied());
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
    mut projection: LayoutProjection,
    geometry_revision: Single<&LayoutGeometryRevision>,
    header_q: Query<(Entity, Has<Open>, Option<&ComputedNode>), With<Header>>,
    side_sheet_q: Query<(Entity, &SideSheetPosition, Has<Open>, &Node), With<SideSheet>>,
    window_q: Query<(&HostWindow, &Node), With<VmuxWindow>>,
    windows: Query<(&Window, Has<LayoutHidden>)>,
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
    let (window_width_px, zen) = windows
        .get(host_window)
        .ok()
        .map(|(window, zen)| (window.resolution.physical_width() as f32, zen))
        .unwrap_or((0.0, false));
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

    let payload = LayoutGeometry {
        zen,
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
        pane_gap: SIDE_SHEET_GAP_PX,
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
        .trigger(UiStateWrite::<UiLayout>::from_event(cef_e, &payload));
}

struct AddressRoots<'a> {
    repos: Option<&'a mut RepoInfoCache>,
    home: std::path::PathBuf,
}

impl AddressRoots<'_> {
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
        AddressParts::on_disk(&path, &self.home)
    }

    fn checkout_of(&mut self, path: &std::path::Path) -> Option<RepoInfo> {
        let dir = match path.is_dir() {
            true => path,
            false => path.parent()?,
        };
        self.repos.as_mut()?.snapshot(dir)
    }
}

fn push_stacks_host_emit(
    mut projection: LayoutProjection,
    browser_q: Query<
        (
            Entity,
            &PageMetadata,
            &ChildOf,
            Option<&NavigationState>,
            Option<&HostHistory>,
            Option<&PageIdentity>,
        ),
        With<Browser>,
    >,
    stacks: Query<(&PageMetadata, Option<&HostHistory>), (With<Stack>, Without<Retiring>)>,
    zoomed_q: Query<(), With<Zoomed>>,
    focus: FocusedStack,
    surfaces: StackSurface,
    retiring: Query<(), With<Retiring>>,
    child_of_q: Query<&ChildOf>,
    mut repo_info: Option<Single<&mut RepoInfoCache>>,
) {
    let Some(target) = projection.target() else {
        return;
    };
    let cef_e = target.entity;
    let active_pane = focus.pane;
    let active_stack_opt = focus.stack.filter(|stack| stacks.contains(*stack));
    let mut rows: Vec<StackRow> = Vec::new();
    let mut can_go_back = false;
    let mut can_go_forward = false;
    let mut roots = AddressRoots {
        repos: repo_info
            .as_mut()
            .map(|cache| cache.bypass_change_detection()),
        home: std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default(),
    };
    if let Some(active_stack_entity) = active_stack_opt {
        let mut active_found = false;
        for (browser, meta, child_of, nav_state, host_history, osc) in &browser_q {
            let stack_entity = child_of.get();
            if retiring.contains(stack_entity) {
                continue;
            }
            if !surfaces.is_current(stack_entity, browser) {
                continue;
            }
            let stack_pane = child_of_q.get(stack_entity).ok().map(|co| co.get());
            if stack_pane != active_pane {
                continue;
            }
            let is_active = stack_entity == active_stack_entity;
            if is_active {
                active_found = true;
                if let Ok((metadata, Some(history))) = stacks.get(stack_entity)
                    && VmuxRoute::parse(&metadata.url).is_some()
                {
                    can_go_back = history.can_go_back();
                    can_go_forward = history.can_go_forward();
                } else if let Some(history) = host_history {
                    can_go_back = history.can_go_back();
                    can_go_forward = history.can_go_forward();
                } else if let Some(ns) = nav_state {
                    can_go_back = ns.can_go_back;
                    can_go_forward = ns.can_go_forward;
                }
            }
            let pending = is_active
                .then(|| stacks.get(stack_entity).ok().map(|(metadata, _)| metadata))
                .flatten();
            let projected = PagePresentation::metadata(pending, Some(meta)).unwrap_or(meta);
            let identity = PagePresentation::identity(projected, Some(meta), osc);
            let title = PagePresentation::title(projected, identity);
            rows.push(StackRow {
                address: roots.resolve(&projected.url, &title),
                title,
                url: projected.url.clone(),
                icon: projected.icon.clone(),
                is_active,
                bg_color: projected.bg_color.clone(),
            });
        }
        if !active_found
            && let Ok((metadata, history)) = stacks.get(active_stack_entity)
            && !metadata.url.is_empty()
        {
            if let Some(history) = history
                && VmuxRoute::parse(&metadata.url).is_some()
            {
                can_go_back = history.can_go_back();
                can_go_forward = history.can_go_forward();
            }
            rows.push(StackRow {
                address: roots.resolve(&metadata.url, &metadata.title),
                title: metadata.title.clone(),
                url: metadata.url.clone(),
                icon: metadata.icon.clone(),
                is_active: true,
                bg_color: metadata.bg_color.clone(),
            });
        }
    }
    let is_zoomed = focus.tab.map(|t| zoomed_q.get(t).is_ok()).unwrap_or(false);
    let payload = StackNavigationState {
        stacks: rows,
        can_go_back,
        can_go_forward,
        is_zoomed,
    };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, ron_body) {
        return;
    }
    projection
        .commands
        .entity(cef_e)
        .insert(StackProjection(payload.clone()));
    projection
        .commands
        .trigger(UiStateWrite::<UiLayout>::from_event(cef_e, &payload));
}

fn push_pane_tree_emit(
    mut projection: LayoutProjection,
    focus: FocusedStack,
    layout_focus: LayoutFocus,
    tab_q: Query<(), With<Tab>>,
    sections_of: SideSheetSections,
    collapsed_panes: Query<(), With<SideSheetCardCollapsed>>,
    stack_q: Query<Entity, (With<Stack>, Without<Retiring>)>,
    stack_metadata: Query<&PageMetadata, (With<Stack>, Without<Retiring>)>,
    stack_children: Query<&Children>,
    surfaces: StackSurface,
    browser_meta: Query<
        (
            &PageMetadata,
            Has<Loading>,
            Option<&PageIdentity>,
            Option<&GitDiffSource>,
            Has<vmux_ecs::team::Agent>,
        ),
        With<Browser>,
    >,
) {
    let Some(target) = projection.target() else {
        return;
    };
    let cef_e = target.entity;

    let active_pane = focus.pane;

    let Some(tab_e) = focus.tab else {
        return;
    };
    if !tab_q.contains(tab_e) {
        return;
    }
    let sections = sections_of.under(tab_e);
    let tab_leaf_panes = layout_focus.leaves(tab_e);

    let mut panes: Vec<PaneNode> = Vec::new();
    for &pane_entity in &tab_leaf_panes {
        let is_active = active_pane == Some(pane_entity);
        let active_stack = layout_focus.stack(pane_entity);
        let mut stacks: Vec<StackNode> = Vec::new();
        if let Ok(children) = stack_children.get(pane_entity) {
            for child in children.iter() {
                if !stack_q.contains(child) {
                    continue;
                }
                let stack_is_active = active_stack == Some(child);
                let pending_metadata = stack_metadata
                    .get(child)
                    .ok()
                    .filter(|metadata| !metadata.url.is_empty());
                let mut found_browser = false;
                if let Ok(stack_kids) = stack_children.get(child) {
                    for browser_e in stack_kids.iter() {
                        if !surfaces.is_current(child, browser_e) {
                            continue;
                        }
                        if let Ok((meta, loading, osc, diff, is_agent)) =
                            browser_meta.get(browser_e)
                        {
                            let projected =
                                PagePresentation::metadata(pending_metadata, Some(meta))
                                    .unwrap_or(meta);
                            let same_page = projected.url == meta.url;
                            stacks.push(StackNode {
                                id: child.to_bits(),
                                agent_id: (is_agent && same_page)
                                    .then(|| browser_e.to_bits().to_string()),
                                title: PagePresentation::title(
                                    projected,
                                    same_page.then_some(osc).flatten(),
                                ),
                                url: projected.url.clone(),
                                icon: projected.icon.clone(),
                                is_active: stack_is_active,
                                is_loading: loading || !same_page,
                                is_dirty: same_page && diff.is_some_and(|source| source.dirty),
                                bg_color: projected.bg_color.clone(),
                            });
                            found_browser = true;
                        }
                    }
                }
                if !found_browser {
                    match pending_metadata {
                        Some(metadata) => stacks.push(StackNode {
                            id: child.to_bits(),
                            agent_id: None,
                            title: metadata.title.clone(),
                            url: metadata.url.clone(),
                            icon: metadata.icon.clone(),
                            is_active: stack_is_active,
                            is_loading: true,
                            is_dirty: false,
                            bg_color: metadata.bg_color.clone(),
                        }),
                        None => stacks.push(StackNode {
                            id: child.to_bits(),
                            agent_id: None,
                            title: "New Stack".to_string(),
                            url: String::new(),
                            icon: PageIcon::None,
                            is_active: stack_is_active,
                            is_loading: false,
                            is_dirty: false,
                            bg_color: None,
                        }),
                    }
                }
            }
        }
        panes.push(PaneNode {
            id: pane_entity.to_bits(),
            is_active,
            collapsed: collapsed_panes.contains(pane_entity),
            bookmarks_expanded: sections.bookmarks,
            stacks,
        });
    }
    let payload = PaneTreeState { panes };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, ron_body) {
        return;
    }
    projection
        .commands
        .entity(cef_e)
        .insert(PaneTreeProjection(payload.clone()));

    projection
        .commands
        .trigger(UiStateWrite::<UiLayout>::from_event(cef_e, &payload));
}

fn push_projects_host_emit(
    mut projection: LayoutProjection,
    focus: FocusedStack,
    space_projects: SpaceProjects,
    active_projects: Res<vmux_space::ActiveProjectRows>,
    pending_selections: Query<&vmux_layout::page_context::PendingWorkspaceSelection>,
    worktrees: Query<&vmux_layout::tab::TabWorktree>,
    mut repo_info: Option<Single<&mut RepoInfoCache>>,
    mut cursor: Local<ProjectProjectionCursor>,
) {
    let selection = focus.tab.and_then(|tab| {
        let pending = pending_selections.get(tab).ok()?;
        Some(ProjectProjectionSelection {
            tab,
            revision: pending.revision(),
        })
    });
    let Some((layout, _)) = projection.layout.get() else {
        return;
    };
    let selection_changed = cursor.needs_emit(layout, &selection);
    let Some(target) = projection.target_for(selection_changed) else {
        return;
    };
    let cef_e = target.entity;
    let source = space_projects.active_rows();
    let mut projects = active_projects.layout_rows(space_projects.active_space_id(), &source);
    for row in &mut projects {
        let display_path = match row.display_path.is_empty() {
            true => &row.path,
            false => &row.display_path,
        };
        row.display_path = vmux_path::DisplayPath::project(display_path);
    }
    let mut boundary = projects
        .iter()
        .find(|project| project.depth == 0 && project.is_active)
        .map(|project| TabBoundary {
            effective_dir: project.path.clone(),
            project_dir: project.path.clone(),
            source: "project".to_string(),
            branch: project.branch.clone(),
            ..Default::default()
        });
    let mut branches = Vec::new();
    if let Some(cache) = repo_info.as_mut() {
        let cache = cache.bypass_change_detection();
        cache.release(cef_e);
        for row in &mut projects {
            if row.missing || row.kind != vmux_api::space::ProjectRowKind::Project {
                continue;
            }
            let path = std::path::Path::new(&row.path);
            let info = row
                .is_active
                .then(|| cache.observe_active(cef_e, path))
                .flatten();
            if let Some(info) = info {
                row.branch = info.branch.clone();
                if row.is_active {
                    let repository = info.project_name();
                    let path = std::path::Path::new(&row.path);
                    let Ok(relative) = path.strip_prefix(&info.repo_root) else {
                        continue;
                    };
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
                    boundary = Some(TabBoundary {
                        effective_dir: row.path.clone(),
                        project_dir: project_dir.to_string_lossy().into_owned(),
                        source: "project".to_string(),
                        repository,
                        is_git_repo: true,
                        is_worktree: info.is_worktree,
                        branch: info.branch,
                        base_ref: info.base_ref,
                        uncommitted: info.uncommitted,
                        ahead: info.ahead,
                        changed_files: info.changed_files,
                        insertions: info.insertions,
                        deletions: info.deletions,
                        ..Default::default()
                    });
                }
            }
        }
    }
    if let Some(tab) = focus.tab
        && let Ok(worktree) = worktrees.get(tab)
    {
        if let Some(project) = projects.iter_mut().find(|project| project.is_active) {
            project.branch.clone_from(&worktree.branch);
        }
        if let Some(boundary) = boundary.as_mut() {
            boundary.branch.clone_from(&worktree.branch);
        }
    }
    if let Some(tab) = focus.tab
        && let Ok(pending) = pending_selections.get(tab)
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
    let payload = TabBoundaryState {
        boundary,
        projects,
        branches,
    };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    cursor.record(target.entity, selection);
    if !projection.should_emit(target, ron_body) {
        return;
    }
    projection
        .commands
        .entity(cef_e)
        .insert(ProjectProjection(payload.clone()));
    projection
        .commands
        .trigger(UiStateWrite::<UiLayout>::from_event(cef_e, &payload));
}

fn push_bookmarks_host_emit(
    mut projection: LayoutProjection,
    pins: Query<
        (
            &Id<BookmarkEntry>,
            &PageMetadata,
            &BookmarkOrder,
            Has<Bookmark>,
        ),
        With<Pin>,
    >,
    folders: Query<
        (
            Entity,
            &Id<BookmarkEntry>,
            &Name,
            Option<&Children>,
            Has<Collapsed>,
            Option<&SmartBookmarkFolder>,
            &BookmarkOrder,
            Option<&ChildOf>,
        ),
        With<Folder>,
    >,
    top_bookmarks: Query<
        (&Id<BookmarkEntry>, &PageMetadata, &BookmarkOrder, Has<Pin>),
        (With<Bookmark>, Without<ChildOf>),
    >,
    child_bookmarks: Query<
        (&Id<BookmarkEntry>, &PageMetadata, &BookmarkOrder, Has<Pin>),
        With<Bookmark>,
    >,
) {
    let Some(target) = projection.target() else {
        return;
    };
    let cef_e = target.entity;

    let row = |uuid: &Id<BookmarkEntry>, meta: &PageMetadata, bookmarked: bool, pinned: bool| {
        BookmarkRow {
            uuid: uuid.as_str().to_string(),
            metadata: meta.clone(),
            bookmarked,
            pinned,
        }
    };

    let mut pin_entries: Vec<(u32, BookmarkRow)> = pins
        .iter()
        .map(|(u, m, o, bookmarked)| (o.0, row(u, m, bookmarked, true)))
        .collect();
    pin_entries.sort_by_key(|(order, _)| *order);
    let pin_rows: Vec<BookmarkRow> = pin_entries.into_iter().map(|(_, r)| r).collect();

    let mut roots: Vec<(u32, BookmarkNode)> = Vec::new();
    for (_, uuid, name, children, collapsed, smart, order, parent) in folders.iter() {
        if smart.is_some() {
            continue;
        }
        let mut kids = Vec::new();
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok((uuid, meta, order, pinned)) = child_bookmarks.get(child) {
                    kids.push((order.0, row(uuid, meta, true, pinned)));
                }
            }
        }
        kids.sort_by_key(|(order, _)| *order);
        let parent = parent.and_then(|parent| {
            folders
                .get(parent.get())
                .ok()
                .map(|(_, uuid, _, _, _, _, _, _)| uuid.as_str().to_string())
        });
        roots.push((
            order.0,
            BookmarkNode::Folder(BookmarkFolderRow {
                uuid: uuid.as_str().to_string(),
                name: name.as_str().to_string(),
                collapsed,
                parent,
                children: kids.into_iter().map(|(_, row)| row).collect(),
            }),
        ));
    }
    for (uuid, meta, order, pinned) in top_bookmarks.iter() {
        roots.push((order.0, BookmarkNode::Entry(row(uuid, meta, true, pinned))));
    }
    roots.sort_by_key(|(o, _)| *o);
    let roots: Vec<BookmarkNode> = roots.into_iter().map(|(_, n)| n).collect();

    let folders = BookmarkFolders::from_nodes(&roots).0;
    let payload = BookmarkStateEvent {
        pins: pin_rows,
        roots,
        folders,
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, body) {
        return;
    }
    projection
        .commands
        .entity(cef_e)
        .insert(BookmarkProjection(payload.clone()));
    projection
        .commands
        .trigger(UiStateWrite::<UiLayout>::from_event(cef_e, &payload));
}

fn push_tabs_host_emit(mut projection: LayoutProjection, tabs: TabProjectionData) {
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
        .trigger(UiStateWrite::<UiLayout>::from_event(cef_e, &payload));
}

fn push_update_notice_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    layout: FocusedLayout,
    state: Single<&UpdateState>,
    mut last: Local<std::collections::HashMap<Entity, UpdateState>>,
) {
    let Some((cef_e, page_ready_changed)) = layout.get() else {
        return;
    };
    if !browsers.can_emit_to(&cef_e) {
        return;
    }
    let previous = last.get(&cef_e).cloned();
    if !UpdateProjection::should_emit(&state, &previous, page_ready_changed) {
        return;
    }
    match &*state {
        UpdateState::Idle => {
            commands.trigger(UiStateWrite::<UiLayout>::from_event(cef_e, &UpdateCleared));
        }
        UpdateState::Downloading {
            version,
            downloaded,
            total,
        } => {
            commands.trigger(UiStateWrite::<UiLayout>::from_event(
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
            commands.trigger(UiStateWrite::<UiLayout>::from_event(
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
            commands.trigger(UiStateWrite::<UiLayout>::from_event(
                cef_e,
                &UpdateReady {
                    version: version.clone(),
                },
            ));
        }
    }
    last.insert(cef_e, state.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

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

    #[test]
    fn project_projection_selection_stays_dirty_until_emitted() {
        let layout = Entity::from_bits(1);
        let tab = Entity::from_bits(2);
        let selection = Some(ProjectProjectionSelection { tab, revision: 1 });
        let mut cursor = ProjectProjectionCursor::default();

        assert!(cursor.needs_emit(layout, &selection));
        assert!(cursor.needs_emit(layout, &selection));

        cursor.record(layout, selection.clone());

        assert!(!cursor.needs_emit(layout, &selection));
        assert!(cursor.needs_emit(layout, &None));
    }

    #[test]
    fn projection_cache_rebuilds_once_per_revision() {
        let entity = Entity::from_bits(1);
        let mut cache = ProjectionCache::default();

        assert!(cache.needs_rebuild(entity, 0, false));
        assert!(cache.should_emit(entity, 0, "body".to_string(), false));
        assert!(!cache.needs_rebuild(entity, 0, false));

        assert!(cache.needs_rebuild(entity, 1, false));
        assert!(!cache.should_emit(entity, 1, "body".to_string(), false));
        assert!(!cache.needs_rebuild(entity, 1, false));
    }

    #[test]
    fn page_ready_forces_cached_projection_emission() {
        let entity = Entity::from_bits(1);
        let mut cache = ProjectionCache::default();

        assert!(cache.should_emit(entity, 0, "body".to_string(), false));
        assert!(cache.needs_rebuild(entity, 0, true));
        assert!(cache.should_emit(entity, 0, "body".to_string(), true));
    }
}
