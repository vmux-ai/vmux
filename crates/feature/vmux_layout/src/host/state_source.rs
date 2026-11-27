use std::collections::HashSet;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use bevy::ecs::relationship::Relationship;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, HostWindow};
use vmux_api::bookmark::{
    BookmarkFolderChoice, BookmarkFolderRow, BookmarkNode, BookmarkRow, BookmarkStateEvent,
    SmartBookmarkFolder,
};
use vmux_api::{PageIcon, VmuxRoute};
use vmux_ecs::notify::AgentDoneUnseen;
use vmux_ecs::page::{HostHistory, PageReady};
use vmux_ecs::surface::SurfaceReady;
use vmux_ecs::{
    Active, Bookmark, BookmarkOrder, Collapsed, Folder, PageIdentity, PageMetadata, Pin,
    UiStateFlushSet, UiStateWrite, Uuid,
};
use vmux_flex::prelude::*;
use vmux_git::worktree::RepoInfo;
use vmux_git::{GitDiffSource, RepoInfoCache};
use vmux_history::LastActivatedAt;

use crate::cef::{Browser, LayoutCef};
use crate::event::{
    AddressParts, HEADER_HEIGHT_PX, LayoutGeometry, PaneNode, PaneTreeState, SIDE_SHEET_GAP_PX,
    SideSheetResizeEvent, StackNavigationState, StackNode, StackRow, TabListState, TabRow,
    UpdateCleared, UpdateProgress, UpdateReady,
};
use crate::hosted_page::StackSurface;
use crate::page_slot::Current;
use crate::pane::{Pane, PaneSplit, SideSheetCardCollapsed, Zoomed};
use crate::presentation::{PresentationFlushSet, Retiring};
use crate::projection::{
    BookmarkProjection, LayoutUiProjectionSet, PaneTreeProjection, StackProjection,
};
use crate::settings::LayoutSettings;
use crate::side_sheet::{
    SideSheet, SideSheetPosition, SideSheetSections, SideSheetSectionsExpanded,
};
use crate::space::{CurrentSpace, Space, SpaceId};
use crate::stack::{ActiveTabParam, FocusedStack, LayoutFocus, Stack};
use crate::state::LayoutUiState;
use crate::tab::{Tab, TabHierarchy};
use crate::toggle::{LayoutHidden, UrlBarHidden};
use crate::window::{FocusedWindow, VmuxWindow, WindowHierarchy};
use crate::{Header, Loading, NavigationState, Open, UpdateStage, UpdateState};

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutStateSourceSet {
    MarkDirty,
}

pub struct LayoutStateSourcePlugin;

impl Plugin for LayoutStateSourcePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(retire_receipts)
            .add_observer(retire_window_geometry)
            .add_observer(retire_window_focus)
            .add_observer(retire_update_notice)
            .add_systems(Startup, spawn_revision)
            .configure_sets(
                PostUpdate,
                LayoutStateSourceSet::MarkDirty
                    .after(PresentationFlushSet)
                    .before(LayoutUiProjectionSet::Source),
            )
            .add_systems(
                PostUpdate,
                mark_dirty.in_set(LayoutStateSourceSet::MarkDirty),
            )
            .add_systems(
                PostUpdate,
                (
                    push_stacks,
                    push_pane_tree,
                    push_tabs,
                    push_bookmarks,
                    push_update_notice,
                )
                    .in_set(LayoutUiProjectionSet::Source),
            )
            .add_systems(
                PostUpdate,
                (mark_computed_dirty, push_geometry)
                    .chain()
                    .before(UiStateFlushSet)
                    .after(LayoutSystems::Layout)
                    .after(LayoutStateSourceSet::MarkDirty),
            );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutWindowPadding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl LayoutWindowPadding {
    pub fn from_node(node: &Node) -> Self {
        Self {
            top: Self::px(node.padding.top),
            right: Self::px(node.padding.right),
            bottom: Self::px(node.padding.bottom),
            left: Self::px(node.padding.left),
        }
    }

    pub fn from_settings(settings: &LayoutSettings) -> Self {
        Self {
            top: settings.window.pad_top(),
            right: settings.window.pad_right(),
            bottom: settings.window.pad_bottom(),
            left: settings.window.pad_left(),
        }
    }

    fn px(value: Val) -> f32 {
        match value {
            Val::Px(px) => px,
            _ => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutFixedOffsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub height: f32,
}

impl LayoutFixedOffsets {
    pub fn from_node(rect: &ComputedNode, window_width_px: f32) -> Option<Self> {
        if rect.is_empty() || window_width_px <= 0.0 {
            return None;
        }
        let logical = rect.to_logical();
        let window_width = window_width_px * rect.inverse_scale_factor.max(1.0e-6);
        Some(Self {
            left: logical.min().x,
            top: logical.min().y,
            right: window_width - logical.max().x,
            height: logical.size.y,
        })
    }
}

pub struct PagePresentation;

impl PagePresentation {
    pub fn title(metadata: &PageMetadata, identity: Option<&PageIdentity>) -> String {
        match identity.and_then(|identity| identity.title.as_deref()) {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => metadata.title.clone(),
        }
    }

    pub fn metadata<'a>(
        pending: Option<&'a PageMetadata>,
        presented: Option<&'a PageMetadata>,
    ) -> Option<&'a PageMetadata> {
        pending
            .filter(|metadata| !metadata.url.is_empty())
            .or(presented)
    }

    pub fn identity<'a>(
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

#[derive(SystemParam)]
pub struct TabProjectionData<'w, 's> {
    tabs: Query<'w, 's, (Entity, &'static Tab, &'static LastActivatedAt), Without<Retiring>>,
    hierarchy: TabHierarchy<'w, 's>,
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

    pub fn rows(&self) -> Vec<TabRow> {
        let active_tab = self.active_tab.get();
        let done_tabs = self
            .done_agents
            .iter()
            .filter_map(|agent| self.hierarchy.entity(agent))
            .collect::<HashSet<_>>();
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

#[derive(SystemParam)]
pub struct FocusedLayout<'w, 's> {
    focused: FocusedWindow<'w, 's>,
    layouts: Query<'w, 's, (Entity, Ref<'static, PageReady>), With<LayoutCef>>,
    pub hierarchy: WindowHierarchy<'w, 's>,
}

impl FocusedLayout<'_, '_> {
    pub fn get(&self) -> Option<(Entity, bool)> {
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
pub struct StateRevision(pub u64);

#[derive(Component, Default)]
pub struct LayoutGeometryRevision(pub u64);

impl StateRevision {
    pub fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

impl LayoutGeometryRevision {
    pub fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

#[derive(Component)]
pub struct ProjectionReceipt<T: Send + Sync + 'static> {
    revision: u64,
    body: String,
    payload: PhantomData<fn() -> T>,
}

#[allow(unknown_lints, vmux_ecs_ownership)]
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
        Self::collect(&folders, None, "", &[], &mut HashSet::new(), &mut output);
        Self(output)
    }

    fn collect(
        folders: &[BookmarkFolderRow],
        parent: Option<&str>,
        parent_label: &str,
        ancestors: &[String],
        visited: &mut HashSet<String>,
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

#[derive(SystemParam)]
pub struct LayoutDelivery<'w, 's> {
    browsers: Option<NonSend<'w, Browsers>>,
    layouts: Query<'w, 's, Has<SurfaceReady>, With<LayoutCef>>,
}

impl LayoutDelivery<'_, '_> {
    pub fn available(&self, entity: Entity) -> bool {
        let Ok(surface_ready) = self.layouts.get(entity) else {
            return false;
        };
        surface_ready
            || self
                .browsers
                .as_ref()
                .is_some_and(|browsers| browsers.can_emit_to(&entity))
    }
}

#[derive(SystemParam)]
pub struct LayoutProjection<'w, 's, T: Send + Sync + 'static> {
    pub commands: Commands<'w, 's>,
    delivery: LayoutDelivery<'w, 's>,
    pub layout: FocusedLayout<'w, 's>,
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
pub struct ProjectionTarget {
    pub entity: Entity,
    revision: u64,
    page_ready_changed: bool,
}

impl<T: Send + Sync + 'static> LayoutProjection<'_, '_, T> {
    pub fn target(&self) -> Option<ProjectionTarget> {
        self.target_for_revision(self.revision.0, false)
    }

    pub fn target_for(&self, force: bool) -> Option<ProjectionTarget> {
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

    pub fn should_emit(&mut self, target: ProjectionTarget, body: String) -> bool {
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
                payload: PhantomData,
            });
        changed
    }
}

fn retire_receipts(removed: On<Remove, LayoutCef>, mut commands: Commands) {
    commands.entity(removed.event_target()).try_remove::<(
        ProjectionReceipt<LayoutGeometry>,
        ProjectionReceipt<StackNavigationState>,
        ProjectionReceipt<PaneTreeState>,
        ProjectionReceipt<BookmarkStateEvent>,
        ProjectionReceipt<TabListState>,
    )>();
}

type PageProjectionChanged = Or<(
    Changed<Current>,
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
    Changed<SurfaceReady>,
    Changed<SideSheet>,
    Changed<VmuxWindow>,
    Changed<Stack>,
    Changed<Pane>,
    Changed<PaneSplit>,
    Changed<Zoomed>,
    Changed<Space>,
    Changed<CurrentSpace>,
    Changed<Active>,
    Or<(
        Changed<LayoutHidden>,
        Changed<UrlBarHidden>,
        Changed<Retiring>,
    )>,
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

#[derive(SystemParam)]
struct StateChanges<'w, 's> {
    pages: Query<'w, 's, (), PageProjectionChanged>,
    layout: Query<'w, 's, (), LayoutProjectionChanged>,
    markers: Query<'w, 's, (), LayoutMarkerChanged>,
    bookmarks: Query<'w, 's, (), BookmarkProjectionChanged>,
}

#[derive(SystemParam)]
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
    }
}

#[derive(SystemParam)]
struct PageStateRemovals<'w, 's> {
    current: RemovedComponents<'w, 's, Current>,
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
        state |= self.retiring.read().count() > 0;
        PageStateRemovalChanges { state, geometry }
    }
}

#[derive(Clone, Copy)]
struct PageStateRemovalChanges {
    state: bool,
    geometry: bool,
}

fn mark_dirty(
    changes: StateChanges,
    geometry: LayoutGeometryChanges,
    mut removals: PageStateRemovals,
    focused_window: FocusedWindow,
    focused_stack: FocusedStack,
    settings: Option<Res<LayoutSettings>>,
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
    let settings_changed = settings.as_ref().is_some_and(|s| s.is_changed());
    let resource_changed = focused_window_changed
        || focused_stack.is_changed()
        || settings_changed
        || repo_info.as_ref().is_some_and(|value| value.is_changed());
    if resource_changed || changes.any() || removals.state {
        revision.advance();
    }
    if focused_window_changed
        || settings_changed
        || window_geometry_changed
        || geometry.any()
        || removals.geometry
    {
        geometry_revision.advance();
    }
}

fn mark_computed_dirty(
    headers: Query<(), (With<Header>, Changed<ComputedNode>)>,
    mut geometry_revision: Single<&mut LayoutGeometryRevision>,
) {
    if !headers.is_empty() {
        geometry_revision.advance();
    }
}

fn push_geometry(
    mut projection: LayoutProjection<LayoutGeometry>,
    geometry_revision: Single<&LayoutGeometryRevision>,
    header_q: Query<(Entity, Has<Open>, Option<&ComputedNode>), With<Header>>,
    side_sheet_q: Query<(Entity, &SideSheetPosition, Has<Open>, &Node), With<SideSheet>>,
    window_q: Query<(&HostWindow, &Node), With<VmuxWindow>>,
    windows: Query<(&Window, Has<LayoutHidden>, Has<UrlBarHidden>)>,
    settings: Option<Res<LayoutSettings>>,
) {
    let Some(target) = projection.target_for_revision(geometry_revision.0, false) else {
        return;
    };
    let cef_e = target.entity;
    let Some(host_window) = projection.layout.hierarchy.get(cef_e) else {
        return;
    };
    let default_settings = LayoutSettings::default();
    let settings = settings.as_deref().unwrap_or(&default_settings);
    let window_padding = window_q
        .iter()
        .find_map(|(host, node)| (host.0 == host_window).then_some(node))
        .map(LayoutWindowPadding::from_node)
        .unwrap_or_else(|| LayoutWindowPadding::from_settings(settings));
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
    let mut side_sheet_width = SideSheetResizeEvent::live(settings.side_sheet.width).clamped();
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
        side_sheet_content_width: SideSheetResizeEvent::live(settings.side_sheet.width).clamped(),
        pane_gap,
        radius: settings.radius,
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

#[derive(SystemParam)]
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
            .map(PathBuf::from)
            .unwrap_or_default();
        AddressParts::on_disk(&path, &home)
    }

    fn checkout_of(&mut self, path: &Path) -> Option<RepoInfo> {
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

#[derive(SystemParam)]
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

fn push_stacks(mut projection: LayoutProjection<StackNavigationState>, mut rows: NavigationRows) {
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

#[derive(SystemParam)]
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

fn push_pane_tree(mut projection: LayoutProjection<PaneTreeState>, rows: PaneRows) {
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

#[derive(SystemParam)]
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

fn push_bookmarks(mut projection: LayoutProjection<BookmarkStateEvent>, rows: BookmarkRows) {
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

fn push_tabs(mut projection: LayoutProjection<TabListState>, tabs: TabProjectionData) {
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
pub struct UpdateNoticeReceipt {
    stage: UpdateStage,
    version: String,
    downloaded: u64,
    total: u64,
}

impl UpdateNoticeReceipt {
    fn from_state(state: &UpdateState) -> Self {
        Self {
            stage: state.stage,
            version: state.version.clone(),
            downloaded: state.downloaded,
            total: state.total,
        }
    }

    fn matches(&self, state: &UpdateState) -> bool {
        self.stage == state.stage
            && self.version == state.version
            && self.downloaded == state.downloaded
            && self.total == state.total
    }
}

fn retire_update_notice(removed: On<Remove, LayoutCef>, mut commands: Commands) {
    commands
        .entity(removed.event_target())
        .try_remove::<UpdateNoticeReceipt>();
}

fn push_update_notice(
    mut commands: Commands,
    delivery: LayoutDelivery,
    layout: FocusedLayout,
    state: Option<Single<&UpdateState>>,
    receipts: Query<&UpdateNoticeReceipt, With<LayoutCef>>,
) {
    let Some(state) = state else {
        return;
    };
    let Some((cef_e, page_ready_changed)) = layout.get() else {
        return;
    };
    if !delivery.available(cef_e) {
        return;
    }
    let unchanged = receipts
        .get(cef_e)
        .is_ok_and(|previous| previous.matches(&state));
    if unchanged && (!page_ready_changed || state.is_idle()) {
        return;
    }
    match state.stage {
        UpdateStage::Idle => {
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                cef_e,
                &UpdateCleared,
            ));
        }
        UpdateStage::Downloading => {
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                cef_e,
                &UpdateProgress {
                    version: state.version.clone(),
                    downloaded: state.downloaded,
                    total: state.total,
                    installing: false,
                },
            ));
        }
        UpdateStage::Installing => {
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                cef_e,
                &UpdateProgress {
                    version: state.version.clone(),
                    downloaded: 0,
                    total: 0,
                    installing: true,
                },
            ));
        }
        UpdateStage::Ready => {
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                cef_e,
                &UpdateReady {
                    version: state.version.clone(),
                },
            ));
        }
    }
    commands
        .entity(cef_e)
        .insert(UpdateNoticeReceipt::from_state(&state));
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
    fn computed_header_resize_invalidates_layout_geometry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, mark_computed_dirty);
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
    fn window_focus_does_not_dirty_layout_geometry() {
        let mut window = Window::default();
        let before = WindowGeometryState::of(&window);
        window.focused = !window.focused;

        assert_eq!(WindowGeometryState::of(&window), before);
    }

    #[test]
    fn surface_ready_layout_emits_without_cef_browsers() {
        #[derive(Resource, Default)]
        struct Seen {
            tabs: bool,
            panes: bool,
            layout: bool,
        }

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutStateSourcePlugin))
            .init_resource::<Seen>()
            .add_observer(
                |write: On<UiStateWrite<LayoutUiState>>, mut seen: ResMut<Seen>| {
                    let patch = write.update();
                    seen.tabs |= patch.tabs.is_some();
                    seen.panes |= patch.pane_tree.is_some();
                    seen.layout |= patch.layout.is_some();
                },
            );
        let window = app.world_mut().spawn((Window::default(), Active)).id();
        let root = app.world_mut().spawn(HostWindow(window)).id();
        app.world_mut()
            .spawn((LayoutCef, PageReady {}, SurfaceReady, ChildOf(root)));
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(root)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, Active, LastActivatedAt(1), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                Active,
                LastActivatedAt(1),
                ChildOf(pane),
                PageMetadata {
                    title: "Terminal".into(),
                    url: "vmux://terminal/".into(),
                    ..Default::default()
                },
            ))
            .id();
        let active = crate::active_pane::ActiveStack {
            tab: Some(tab),
            pane: Some(pane),
            stack: Some(stack),
        };
        app.world_mut().spawn(active.local_bundle());

        app.update();

        let seen = app.world().resource::<Seen>();
        assert!(seen.tabs);
        assert!(seen.panes);
        assert!(seen.layout);
    }
}
