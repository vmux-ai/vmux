use bevy::{ecs::entity::EntityHashMap, ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use vmux_api::VmuxRoute;
use vmux_api::bookmark::{
    BookmarkFolderChoice, BookmarkFolderRow, BookmarkNode, BookmarkRow, BookmarkStateEvent,
    SmartBookmarkFolder,
};
use vmux_ecs::{
    Active, Bookmark, BookmarkOrder, Collapsed, Folder, PageIcon, PageIdentity, PageMetadata, Pin,
    Uuid,
    host::UiStateWrite,
    notify::AgentDoneUnseen,
    page::{HostHistory, PageReady},
};
use vmux_git::worktree::RepoInfo;
use vmux_git::{GitDiffSource, RepoInfoCache};
use vmux_history::LastActivatedAt;
use vmux_layout::projection::{
    BookmarkProjection, PaneTreeProjection, ProjectProjection, StackProjection,
};
use vmux_layout::{Browser, Loading};
use vmux_layout::{
    Header, LayoutCef, NavigationState, Open, UpdateState,
    event::{
        AddressParts, HEADER_HEIGHT_PX, LayoutGeometry, PANE_GAP_PX, PaneNode, PaneTreeState,
        StackNavigationState, StackNode, StackRow, TabBoundary, TabBoundaryState, TabListState,
        TabRow, UpdateCleared, UpdateProgress, UpdateReady,
    },
    pane::{Pane, PaneSplit, SideSheetCardCollapsed, Zoomed},
    side_sheet::{SideSheet, SideSheetPosition, SideSheetSections, SideSheetSectionsExpanded},
    space::{CurrentSpace, Space, SpaceId},
    stack::{ActiveTabParam, FocusedStack, LayoutFocus, Stack},
    state::LayoutUiState,
    tab::{Tab, active_tab_siblings},
    window::{FocusedWindow, VmuxWindow, WindowHierarchy},
};

use vmux_setting::AppSettings;
use vmux_space::{ExpandedProjectDirs, SpaceProjects};

use crate::host::{
    LayoutFixedOffsets, layout_window_padding_from_node, layout_window_padding_from_settings,
    should_emit_cached_payload, should_emit_update,
};
use vmux_flex::prelude::*;

pub(crate) struct PagePresentation;

impl PagePresentation {
    pub(crate) fn title(metadata: &PageMetadata, identity: Option<&PageIdentity>) -> String {
        match identity.and_then(|identity| identity.title.as_deref()) {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => metadata.title.clone(),
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct TabProjectionData<'w, 's> {
    tabs: Query<'w, 's, (Entity, &'static Tab, &'static LastActivatedAt)>,
    tab_entities: Query<'w, 's, Entity, With<Tab>>,
    active_tab: ActiveTabParam<'w, 's>,
    child_of: Query<'w, 's, &'static ChildOf>,
    all_children: Query<'w, 's, &'static Children>,
    focus: LayoutFocus<'w, 's>,
    stack_timestamps: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Stack>>,
    stack_children: Query<'w, 's, &'static Children>,
    browser_metadata:
        Query<'w, 's, (&'static PageMetadata, Option<&'static PageIdentity>), With<Browser>>,
    done_agents: Query<'w, 's, Entity, With<AgentDoneUnseen>>,
}

impl TabProjectionData<'_, '_> {
    fn tab_of(&self, start: Entity) -> Option<Entity> {
        let mut entity = start;
        loop {
            if self.tab_entities.contains(entity) {
                return Some(entity);
            }
            let parent = self.child_of.get(entity).ok()?;
            entity = parent.get();
        }
    }

    fn active_stack(&self, tab: Entity) -> Option<Entity> {
        self.focus
            .leaves(tab)
            .into_iter()
            .filter_map(|pane| self.focus.stack(pane))
            .filter_map(|stack| self.stack_timestamps.get(stack).ok())
            .max_by_key(|(_, timestamp)| timestamp.0)
            .map(|(entity, _)| entity)
    }

    fn page(&self, stack: Entity) -> Option<(&PageMetadata, Option<&PageIdentity>)> {
        let children = self.stack_children.get(stack).ok()?;
        children
            .iter()
            .find_map(|child| self.browser_metadata.get(child).ok())
    }

    fn rows(&self) -> Vec<TabRow> {
        let active_tab = self.active_tab.get();
        let done_tabs = self
            .done_agents
            .iter()
            .filter_map(|agent| self.tab_of(agent))
            .collect::<std::collections::HashSet<_>>();
        let ordered = match active_tab {
            Some(anchor) => active_tab_siblings(
                anchor,
                &self.child_of,
                &self.all_children,
                &self.tab_entities,
            ),
            None => Vec::new(),
        };
        let mut rows = Vec::new();
        for entity in ordered {
            let Ok((entity, tab, _)) = self.tabs.get(entity) else {
                continue;
            };
            let page = self.active_stack(entity).and_then(|stack| self.page(stack));
            let title = page
                .map(|(metadata, identity)| PagePresentation::title(metadata, identity))
                .unwrap_or_default();
            let (url, icon, bg_color) = page
                .map(|(metadata, _)| {
                    (
                        metadata.url.clone(),
                        metadata.icon.clone(),
                        metadata.bg_color.clone(),
                    )
                })
                .unwrap_or_default();
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
        let entity = self
            .focused
            .entity()
            .and_then(|window| {
                self.layouts.iter().find_map(|(entity, _)| {
                    (self.hierarchy.get(entity) == Some(window)).then_some(entity)
                })
            })
            .or_else(|| self.layouts.iter().next().map(|(entity, _)| entity))?;
        let (_, ready) = self.layouts.get(entity).ok()?;
        Some((entity, ready.is_changed()))
    }
}

pub(crate) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_revision)
            .add_systems(
                Update,
                mark_page_dirty
                    .after(vmux_layout::LayoutCefStateSet::Apply)
                    .after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                Update,
                (
                    push_layout_emit,
                    push_stacks_host_emit,
                    push_pane_tree_emit,
                    push_tabs_host_emit,
                    push_bookmarks_host_emit,
                    push_update_notice_emit,
                    push_projects_host_emit,
                )
                    .after(mark_page_dirty),
            );
    }
}

fn spawn_revision(mut commands: Commands) {
    commands.spawn((Name::new("Page state revision"), StateRevision::default()));
}

#[derive(Component, Default)]
struct StateRevision(u64);

impl StateRevision {
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
        if !should_emit_cached_payload(&body, previous, page_ready_changed) {
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

impl LayoutProjection<'_, '_> {
    fn target(&self) -> Option<ProjectionTarget> {
        let (entity, page_ready_changed) = self.layout.get()?;
        if !self.browsers.can_emit_to(&entity) {
            return None;
        }
        let target = ProjectionTarget {
            entity,
            revision: self.revision.0,
            page_ready_changed,
        };
        self.cache
            .needs_rebuild(entity, target.revision, page_ready_changed)
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
    Changed<Node>,
    Changed<SideSheetPosition>,
    Changed<SideSheetCardCollapsed>,
    Changed<Tab>,
    Changed<Window>,
    Changed<HostWindow>,
    Changed<SpaceId>,
    Changed<SideSheetSectionsExpanded>,
)>;

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
}

impl PageStateRemovals<'_, '_> {
    fn any(&mut self) -> bool {
        let mut any = false;
        any |= self.browser.read().count() > 0;
        any |= self.child_of.read().count() > 0;
        any |= self.children.read().count() > 0;
        any |= self.page_metadata.read().count() > 0;
        any |= self.page_identity.read().count() > 0;
        any |= self.page_ready.read().count() > 0;
        any |= self.navigation.read().count() > 0;
        any |= self.history.read().count() > 0;
        any |= self.loading.read().count() > 0;
        any |= self.git_diff.read().count() > 0;
        any |= self.done.read().count() > 0;
        any |= self.active.read().count() > 0;
        any |= self.header.read().count() > 0;
        any |= self.open.read().count() > 0;
        any |= self.side_sheet.read().count() > 0;
        any |= self.side_sheet_position.read().count() > 0;
        any |= self.collapsed_pane.read().count() > 0;
        any |= self.vmux_window.read().count() > 0;
        any |= self.window.read().count() > 0;
        any |= self.node.read().count() > 0;
        any |= self.layout_cef.read().count() > 0;
        any |= self.host_window.read().count() > 0;
        any |= self.tab.read().count() > 0;
        any |= self.stack.read().count() > 0;
        any |= self.last_activated.read().count() > 0;
        any |= self.pane.read().count() > 0;
        any |= self.pane_split.read().count() > 0;
        any |= self.zoomed.read().count() > 0;
        any |= self.space.read().count() > 0;
        any |= self.space_id.read().count() > 0;
        any |= self.current_space.read().count() > 0;
        any |= self.sections_expanded.read().count() > 0;
        any |= self.uuid.read().count() > 0;
        any |= self.bookmark_order.read().count() > 0;
        any |= self.bookmark.read().count() > 0;
        any |= self.pin.read().count() > 0;
        any |= self.folder.read().count() > 0;
        any |= self.name.read().count() > 0;
        any |= self.collapsed.read().count() > 0;
        any |= self.smart_folder.read().count() > 0;
        any |= self.expanded_projects.read().count() > 0;
        any
    }
}

fn mark_page_dirty(
    changes: StateChanges,
    mut removals: PageStateRemovals,
    focused_window: FocusedWindow,
    focused_stack: FocusedStack,
    settings: Res<AppSettings>,
    repo_info: Option<Single<Ref<RepoInfoCache>>>,
    mut revision: Single<&mut StateRevision>,
) {
    let resource_changed = focused_window.is_changed()
        || focused_stack.is_changed()
        || settings.is_changed()
        || repo_info.as_ref().is_some_and(|value| value.is_changed());
    if resource_changed || changes.any() || removals.any() {
        revision.advance();
    }
}

fn push_layout_emit(
    mut projection: LayoutProjection,
    header_q: Query<(Entity, Has<Open>, Option<&ComputedNode>), With<Header>>,
    side_sheet_q: Query<(Entity, &SideSheetPosition, Has<Open>, &Node), With<SideSheet>>,
    window_q: Query<(&HostWindow, &Node), With<VmuxWindow>>,
    windows: Query<&Window>,
    settings: Res<AppSettings>,
) {
    let Some(target) = projection.target() else {
        return;
    };
    let cef_e = target.entity;
    let Some(host_window) = projection.layout.hierarchy.get(cef_e) else {
        return;
    };
    let window_padding = window_q
        .iter()
        .find_map(|(host, node)| (host.0 == host_window).then_some(node))
        .map(layout_window_padding_from_node)
        .unwrap_or_else(|| layout_window_padding_from_settings(&settings));
    let header_open = header_q.iter().any(|(entity, is_open, _)| {
        is_open && projection.layout.hierarchy.get(entity) == Some(host_window)
    });
    let window_width_px = windows
        .get(host_window)
        .ok()
        .map(|window| window.resolution.physical_width() as f32)
        .unwrap_or(0.0);
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
        header_open,
        side_sheet_open,
        header_height: header_offsets
            .map(|offsets| offsets.height)
            .unwrap_or(HEADER_HEIGHT_PX),
        side_sheet_width,
        pane_gap: PANE_GAP_PX,
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
        self.repos.as_mut()?.lookup(dir)
    }
}

fn push_stacks_host_emit(
    mut projection: LayoutProjection,
    browser_q: Query<
        (
            &PageMetadata,
            &ChildOf,
            Option<&NavigationState>,
            Option<&HostHistory>,
            Option<&PageIdentity>,
        ),
        With<Browser>,
    >,
    stack_q: Query<(), With<Stack>>,
    zoomed_q: Query<(), With<Zoomed>>,
    focus: FocusedStack,
    child_of_q: Query<&ChildOf>,
    mut repo_info: Option<Single<&mut RepoInfoCache>>,
) {
    let Some(target) = projection.target() else {
        return;
    };
    let cef_e = target.entity;
    let active_pane = focus.pane;
    let active_stack_opt = focus.stack;
    if let Some(active_stack_entity) = active_stack_opt
        && !stack_q.contains(active_stack_entity)
    {
        return;
    }
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
        for (meta, child_of, nav_state, host_history, osc) in &browser_q {
            let stack_entity = child_of.get();
            let stack_pane = child_of_q.get(stack_entity).ok().map(|co| co.get());
            if stack_pane != active_pane {
                continue;
            }
            let is_active = stack_entity == active_stack_entity;
            if is_active {
                if let Some(history) = host_history {
                    can_go_back = history.can_go_back();
                    can_go_forward = history.can_go_forward();
                } else if let Some(ns) = nav_state {
                    can_go_back = ns.can_go_back;
                    can_go_forward = ns.can_go_forward;
                }
            }
            let title = PagePresentation::title(meta, osc);
            rows.push(StackRow {
                address: roots.resolve(&meta.url, &title),
                title,
                url: meta.url.clone(),
                icon: meta.icon.clone(),
                is_active,
                bg_color: meta.bg_color.clone(),
            });
        }
    }
    if active_stack_opt.is_some() && rows.is_empty() {
        return;
    }
    let is_zoomed = focus.tab.map(|t| zoomed_q.get(t).is_ok()).unwrap_or(false);
    let payload = StackNavigationState {
        stacks: rows,
        can_go_back,
        can_go_forward,
        is_zoomed,
    };
    projection
        .commands
        .entity(cef_e)
        .insert(StackProjection(payload.clone()));
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, ron_body) {
        return;
    }
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &payload));
}

fn push_pane_tree_emit(
    mut projection: LayoutProjection,
    focus: FocusedStack,
    layout_focus: LayoutFocus,
    tab_q: Query<(), With<Tab>>,
    sections_of: SideSheetSections,
    collapsed_panes: Query<(), With<SideSheetCardCollapsed>>,
    stack_q: Query<Entity, With<Stack>>,
    stack_children: Query<&Children>,
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
                let mut found_browser = false;
                if let Ok(stack_kids) = stack_children.get(child) {
                    for browser_e in stack_kids.iter() {
                        if let Ok((meta, loading, osc, diff, is_agent)) =
                            browser_meta.get(browser_e)
                        {
                            let is_new_stack = false;
                            stacks.push(StackNode {
                                id: child.to_bits(),
                                agent_id: is_agent.then(|| browser_e.to_bits().to_string()),
                                title: if is_new_stack {
                                    "New Stack".to_string()
                                } else {
                                    PagePresentation::title(meta, osc)
                                },
                                url: if is_new_stack {
                                    String::new()
                                } else {
                                    meta.url.clone()
                                },
                                icon: if is_new_stack {
                                    PageIcon::None
                                } else {
                                    meta.icon.clone()
                                },
                                is_active: stack_is_active,
                                is_loading: loading,
                                is_dirty: diff.is_some_and(|source| source.dirty),
                                bg_color: meta.bg_color.clone(),
                            });
                            found_browser = true;
                        }
                    }
                }
                if !found_browser {
                    stacks.push(StackNode {
                        id: child.to_bits(),
                        agent_id: None,
                        title: "New Stack".to_string(),
                        url: String::new(),
                        icon: PageIcon::None,
                        is_active: stack_is_active,
                        is_loading: false,
                        is_dirty: false,
                        bg_color: None,
                    });
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
    projection
        .commands
        .entity(cef_e)
        .insert(PaneTreeProjection(payload.clone()));
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, ron_body) {
        return;
    }
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &payload));
}

fn abbreviate_project_path(path: &std::path::Path) -> String {
    let path = path.to_string_lossy();
    let Some(home) = std::env::var_os("HOME") else {
        return path.into_owned();
    };
    let home = home.to_string_lossy();
    if home.is_empty() {
        return path.into_owned();
    }
    let Some(rest) = path.strip_prefix(home.as_ref()) else {
        return path.into_owned();
    };
    format!("~{rest}")
}

fn push_projects_host_emit(
    mut projection: LayoutProjection,
    space_projects: SpaceProjects,
    mut repo_info: Option<Single<&mut RepoInfoCache>>,
) {
    let Some(target) = projection.target() else {
        return;
    };
    let cef_e = target.entity;
    let mut projects = space_projects.active_rows();
    for row in &mut projects {
        row.display_path = abbreviate_project_path(std::path::Path::new(&row.path));
    }
    let mut boundary = projects
        .iter()
        .find(|project| project.depth == 0 && project.is_active)
        .map(|project| TabBoundary {
            effective_dir: project.path.clone(),
            source: "project".to_string(),
            branch: project.branch.clone(),
            ..Default::default()
        });
    if let Some(cache) = repo_info.as_mut() {
        let cache = cache.bypass_change_detection();
        for row in &mut projects {
            if row.missing || row.kind != vmux_api::space::ProjectRowKind::Project {
                continue;
            }
            if let Some(info) = cache.lookup(std::path::Path::new(&row.path)) {
                row.branch = info.branch.clone();
                if row.is_active {
                    let repository = info.project_name();
                    boundary = Some(TabBoundary {
                        effective_dir: row.path.clone(),
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
    let payload = TabBoundaryState { boundary, projects };
    projection
        .commands
        .entity(cef_e)
        .insert(ProjectProjection(payload.clone()));
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, ron_body) {
        return;
    }
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &payload));
}

fn push_bookmarks_host_emit(
    mut projection: LayoutProjection,
    pins: Query<(&Uuid, &PageMetadata, &BookmarkOrder, Has<Bookmark>), With<Pin>>,
    folders: Query<
        (
            Entity,
            &Uuid,
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
        (&Uuid, &PageMetadata, &BookmarkOrder, Has<Pin>),
        (With<Bookmark>, Without<ChildOf>),
    >,
    child_bookmarks: Query<(&Uuid, &PageMetadata, &BookmarkOrder, Has<Pin>), With<Bookmark>>,
) {
    let Some(target) = projection.target() else {
        return;
    };
    let cef_e = target.entity;

    let row = |uuid: &Uuid, meta: &PageMetadata, bookmarked: bool, pinned: bool| BookmarkRow {
        uuid: uuid.0.clone(),
        metadata: meta.clone(),
        bookmarked,
        pinned,
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
                .map(|(_, uuid, _, _, _, _, _, _)| uuid.0.clone())
        });
        roots.push((
            order.0,
            BookmarkNode::Folder(BookmarkFolderRow {
                uuid: uuid.0.clone(),
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
    projection
        .commands
        .entity(cef_e)
        .insert(BookmarkProjection(payload.clone()));
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !projection.should_emit(target, body) {
        return;
    }
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &payload));
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
        .trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &payload));
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
    if !should_emit_update(&state, &previous, page_ready_changed) {
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
    last.insert(cef_e, state.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

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
