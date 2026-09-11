#![allow(non_snake_case)]

use std::rc::Rc;

use crate::event::{
    BOOKMARK_MENU_ACTION_EVENT, BOOKMARKS_EVENT, BookmarkContextMenuEvent, BookmarkMenuActionEvent,
    BookmarkNode, BookmarkRow, BookmarkTextInputEvent, BookmarksCommandEvent, BookmarksHostEvent,
    FolderRow, HeaderCommandEvent, LAYOUT_STATE_EVENT, LayoutOverlayEvent, LayoutStateEvent,
    PANE_TREE_EVENT, PaneNode, PaneTreeEvent, RELOAD_EVENT, REMOTE_STATE_EVENT, ReloadEvent,
    RemoteCommandEvent, RemoteCopyEvent, RemotePhase, RemoteStateEvent, STACKS_EVENT, StackNode,
    StackRow, StacksHostEvent, TABS_EVENT, TabDropPlacement, TabRow, TabsCommandEvent,
    TabsHostEvent, WindowDragRegionEvent,
};
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_command::panel::CommandBarPanel;
use vmux_core::event::team::{TEAM_EVENT, TeamCommandEvent, TeamEvent, TeamMemberRow};
use vmux_core::event::{
    EXTENSION_POPUP_EVENT, EXTENSIONS_LIST_EVENT, ExtActionRequest, ExtListRequest,
    ExtOpenManagerRequest, ExtPinRequest, ExtRow, ExtensionPopupAnchor,
    ExtensionPopupBoundsRequest, ExtensionPopupCloseRequest, ExtensionPopupEvent, ExtensionsEvent,
};
use vmux_core::{PageIcon, PageMetadata};
use vmux_ui::components::avatar::Avatar;
use vmux_ui::components::composer_bar::StatusDot;
use vmux_ui::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use vmux_ui::components::icon::Icon;
use vmux_ui::components::inline_edit::{EditableText, InlineEdit};
use vmux_ui::components::progress::{Progress, ProgressIndicator};
use vmux_ui::components::tree_row::{
    SIDEBAR_CARD_CHEVRON_CLOSED, SIDEBAR_CARD_CHEVRON_OPEN, SIDEBAR_TREE_CHEVRON_CLOSED,
    SIDEBAR_TREE_CHEVRON_OPEN, SIDEBAR_TREE_COLUMN, SIDEBAR_TREE_SCROLLER, SidebarTreeChildren,
    SidebarTreeRow, SidebarTreeRowGroup,
};
use vmux_ui::favicon::{Favicon, favicon_src_for_url};
use vmux_ui::hooks::{send, use_event, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{BuiltinIconView, LineIcon, LineIconView, PageIconView};
use vmux_ui::platform::sleep_ms;
use vmux_ui::scroll::ScrollIntoView;
use vmux_ui::util::cn;

#[component]
pub fn Page() -> Element {
    use_theme();

    let mut layout_state = use_signal(LayoutStateEvent::default);
    let mut layout_state_received = use_signal(|| false);
    let layout_listener = use_listener::<LayoutStateEvent, _>(LAYOUT_STATE_EVENT, move |data| {
        layout_state_received.set(true);
        layout_state.set(data);
    });

    let mut stacks_state = use_signal(StacksHostEvent::default);
    let mut stacks_state_received = use_signal(|| false);
    let stacks_listener = use_listener::<StacksHostEvent, _>(STACKS_EVENT, move |data| {
        stacks_state_received.set(true);
        stacks_state.set(data);
    });

    let mut tabs_state = use_signal(TabsHostEvent::default);
    let mut tabs_state_received = use_signal(|| false);
    let tabs_listener = use_listener::<TabsHostEvent, _>(TABS_EVENT, move |data| {
        tabs_state_received.set(true);
        tabs_state.set(data);
    });

    let mut bookmarks_state = use_signal(BookmarksHostEvent::default);
    let _bookmarks_listener = use_listener::<BookmarksHostEvent, _>(BOOKMARKS_EVENT, move |data| {
        bookmarks_state.set(data);
    });
    let bookmark_menu_action = use_event::<BookmarkMenuActionEvent>(
        BOOKMARK_MENU_ACTION_EVENT,
        BookmarkMenuActionEvent::default,
    );
    use_context_provider(|| bookmark_menu_action);

    let mut reload_key = use_signal(|| 0u32);
    let _reload_listener = use_listener::<ReloadEvent, _>(RELOAD_EVENT, move |_| {
        reload_key.set(reload_key() + 1);
    });

    let mut pane_tree_state = use_signal(PaneTreeEvent::default);
    let mut pane_tree_state_received = use_signal(|| false);
    let pane_tree_listener = use_listener::<PaneTreeEvent, _>(PANE_TREE_EVENT, move |data| {
        pane_tree_state_received.set(true);
        pane_tree_state.set(data);
    });

    let mut spaces_state = use_signal(vmux_core::event::space::SpacesListEvent::default);
    let mut spaces_state_received = use_signal(|| false);
    let spaces_listener = use_listener::<vmux_core::event::space::SpacesListEvent, _>(
        vmux_core::event::space::SPACES_LIST_EVENT,
        move |data| {
            spaces_state_received.set(true);
            spaces_state.set(data);
        },
    );

    let projects_state = use_event::<crate::event::TabBoundaryEvent>(
        crate::event::TAB_BOUNDARY_EVENT,
        crate::event::TabBoundaryEvent::default,
    );

    let team_state = use_event::<TeamEvent>(TEAM_EVENT, TeamEvent::default);
    let remote_state = use_event::<RemoteStateEvent>(REMOTE_STATE_EVENT, RemoteStateEvent::default);

    let extensions_state =
        use_event::<ExtensionsEvent>(EXTENSIONS_LIST_EVENT, ExtensionsEvent::default);
    let extension_popup =
        use_event::<ExtensionPopupEvent>(EXTENSION_POPUP_EVENT, ExtensionPopupEvent::default);
    use_effect(move || {
        let _ = send(&ExtListRequest);
    });

    let mut update_phase = use_signal(|| None::<UpdatePhase>);
    let _update_progress_listener = use_listener::<crate::event::UpdateProgressEvent, _>(
        crate::event::UPDATE_PROGRESS_EVENT,
        move |evt| {
            update_phase.set(Some(if evt.installing {
                UpdatePhase::Installing {
                    version: evt.version,
                }
            } else {
                UpdatePhase::Downloading {
                    version: evt.version,
                    downloaded: evt.downloaded,
                    total: evt.total,
                }
            }));
        },
    );
    let _update_ready_listener = use_listener::<crate::event::UpdateReadyEvent, _>(
        crate::event::UPDATE_READY_EVENT,
        move |evt| {
            update_phase.set(Some(UpdatePhase::Ready {
                version: evt.version,
            }))
        },
    );
    let _update_cleared_listener = use_listener::<crate::event::UpdateClearedEvent, _>(
        crate::event::UPDATE_CLEARED_EVENT,
        move |_| update_phase.set(None),
    );

    let state = layout_state();
    let stacks = stacks_state();
    let tabs = tabs_state();
    let projects = projects_state();
    let team = team_state();
    let remote = remote_state();
    let PaneTreeEvent { panes } = pane_tree_state();
    let active_space = spaces_state().spaces.into_iter().find(|s| s.is_active);
    let layout_error = (layout_listener.error)();
    let stacks_error = (stacks_listener.error)();
    let tabs_error = (tabs_listener.error)();
    let pane_tree_error = (pane_tree_listener.error)();
    let spaces_error = (spaces_listener.error)();
    let overlay_ready = layout_overlay_ready(
        &state,
        listener_ready(layout_state_received(), &layout_error),
        listener_ready(stacks_state_received(), &stacks_error),
        listener_ready(tabs_state_received(), &tabs_error),
        listener_ready(pane_tree_state_received(), &pane_tree_error),
        listener_ready(spaces_state_received(), &spaces_error),
    );
    let radius_px = state.radius;
    let reveal = StackReveal::side_sheet(use_signal(|| None::<(u64, u64)>));
    use_effect(move || set_root_radius_px(radius_px));
    use_effect(move || {
        if !layout_state().side_sheet_open {
            reveal.forget();
            return;
        }
        let PaneTreeEvent { panes } = pane_tree_state();
        let Some(target) = ActiveStack::of(&panes) else {
            return;
        };
        reveal.follow(target);
    });
    let host_sheet_width = state.side_sheet_width;
    let sheet_left = state.window_pad_left;
    let mut sheet_width = use_signal(|| host_sheet_width);
    let mut sheet_resizing = use_signal(|| false);
    use_effect(use_reactive!(|host_sheet_width| {
        if !*sheet_resizing.peek() {
            sheet_width.set(host_sheet_width);
        }
    }));
    let side_sheet_vars = format!(
        "--vmux-side-sheet-width:{}px;--vmux-side-sheet-left:{}px;--vmux-side-sheet-top:{}px;--vmux-side-sheet-bottom:{}px;--vmux-side-sheet-pad-top:{}px;",
        sheet_width(),
        state.window_pad_left,
        state.window_pad_top,
        state.window_pad_bottom,
        crate::event::url_bar_top(),
    );
    let header_vars = format!(
        "--vmux-header-top:{}px;--vmux-header-left:{}px;--vmux-header-right:{}px;--vmux-header-height:{}px;--vmux-tab-row-pad-left:{}px;",
        state.header_top(),
        state.header_left(),
        state.header_right(),
        state.header_height,
        state.tab_row_pad_left(),
    );

    rsx! {
        div { class: "fixed inset-0 pointer-events-none text-foreground",
            if overlay_ready && state.side_sheet_open {
                aside {
                    id: "vmux-side-sheet",
                    class: "pointer-events-auto fixed left-[var(--vmux-side-sheet-left)] top-[var(--vmux-side-sheet-top)] bottom-[var(--vmux-side-sheet-bottom)] min-h-0 overflow-visible w-[var(--vmux-side-sheet-width)] pt-[var(--vmux-side-sheet-pad-top)]",
                    style: "{side_sheet_vars}",
                    SideSheetGrab { resizing: sheet_resizing }
                    div { class: "flex h-full min-h-0 flex-col",
                        SideSheetView {
                            panes,
                            active_space,
                            bookmarks: bookmarks_state(),
                            projects: projects.projects.clone(),
                            boundary: projects.boundary,
                            team: team.members.clone(),
                            pane_tree_error: pane_tree_error.clone(),
                        }
                        if let Some(phase) = update_phase() {
                            UpdateNoticeFooter { phase }
                        }
                    }
                }
            }
            if overlay_ready && state.header_visible() {
                div {
                    class: "pointer-events-auto fixed top-[var(--vmux-header-top)] left-[var(--vmux-header-left)] right-[var(--vmux-header-right)] h-[var(--vmux-header-height)]",
                    style: "{header_vars}",
                    HeaderView {
                        stacks_state: stacks,
                        tabs_state: tabs,
                        bookmarks: bookmarks_state(),
                        team: team.members,
                        extensions: extensions_state().extensions,
                        remote,
                        reload_key: reload_key(),
                        stacks_error: stacks_error.clone(),
                        tabs_error: tabs_error.clone(),
                    }
                }
            }
            CommandBarPanel {}
            if !extension_popup().id.is_empty() {
                ExtensionPopupModal { popup: extension_popup }
            }
            if sheet_resizing() {
                div {
                    class: "pointer-events-auto fixed inset-0 z-[900] cursor-col-resize",
                    onmousemove: move |event: Event<MouseData>| {
                        let x = event.client_coordinates().x as f32 - sheet_left;
                        sheet_width.set(crate::event::SideSheetResizeEvent { width: x }.clamped());
                    },
                    onmouseup: move |_| {
                        sheet_resizing.set(false);
                        let _ = send(
                            &crate::event::SideSheetResizeEvent {
                                width: sheet_width(),
                            },
                        );
                    },
                }
            }
        }
    }
}

#[component]
fn ExtensionPopupModal(popup: Signal<ExtensionPopupEvent>) -> Element {
    let current = popup();
    let state = ExtensionPopupState { popup };
    let placement = ExtensionPopupPlacement::of(current.anchor);
    let reporter = ExtensionPopupBoundsReporter {
        region: use_signal(|| None::<Rc<MountedData>>),
    };
    let mounted = reporter;
    let resized = reporter;
    use_effect(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "extension-popup".to_string(),
            active: true,
        });
    });
    use_drop(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "extension-popup".to_string(),
            active: false,
        });
        let _ = send(&ExtensionPopupCloseRequest);
    });

    rsx! {
        div {
            class: "pointer-events-auto fixed inset-0 z-[1000]",
            tabindex: "-1",
            onmounted: move |event: Event<MountedData>| {
                let target = event.data();
                spawn(async move {
                    let _ = target.set_focus(true).await;
                });
            },
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    state.close();
                }
            },
            onpointerdown: move |_| state.close(),
            div {
                key: "{current.id}",
                class: "glass absolute overflow-hidden rounded-xl border border-border/80 bg-background shadow-2xl",
                style: "{placement.style()}",
                onpointerdown: move |event| event.stop_propagation(),
                onmounted: move |event: Event<MountedData>| mounted.mount(event.data()),
                onresize: move |_: Event<ResizeData>| resized.publish(),
                div { class: "pointer-events-none absolute inset-0 animate-pulse bg-gradient-to-br from-foreground/[0.035] via-transparent to-primary/[0.06]" }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ExtensionPopupPlacement {
    right: i32,
    top: i32,
}

impl ExtensionPopupPlacement {
    fn of(anchor: ExtensionPopupAnchor) -> Self {
        Self {
            right: anchor.right.max(48),
            top: anchor.bottom.max(40) + 6,
        }
    }

    fn style(self) -> String {
        format!(
            "right:max(8px,calc(100vw - {}px));top:{}px;width:min(360px,calc(100vw - 16px));height:min(520px,calc(100vh - {}px));",
            self.right,
            self.top,
            self.top + 8
        )
    }
}

#[derive(Clone, Copy)]
struct ExtensionPopupState {
    popup: Signal<ExtensionPopupEvent>,
}

impl ExtensionPopupState {
    fn close(mut self) {
        let _ = send(&LayoutOverlayEvent {
            id: "extension-popup".to_string(),
            active: false,
        });
        let _ = send(&ExtensionPopupCloseRequest);
        self.popup.set(ExtensionPopupEvent::default());
    }
}

#[derive(Clone, Copy)]
struct ExtensionPopupBoundsReporter {
    region: Signal<Option<Rc<MountedData>>>,
}

impl ExtensionPopupBoundsReporter {
    fn mount(mut self, region: Rc<MountedData>) {
        self.region.set(Some(region));
        self.publish();
    }

    fn publish(self) {
        spawn(async move {
            let Some(region) = (self.region)() else {
                return;
            };
            let Ok(rect) = region.get_client_rect().await else {
                return;
            };
            let _ = send(&ExtensionPopupBoundsRequest {
                left: rect.origin.x as f32,
                top: rect.origin.y as f32,
                width: rect.size.width as f32,
                height: rect.size.height as f32,
            });
        });
    }
}

#[component]
fn SideSheetGrab(mut resizing: Signal<bool>) -> Element {
    rsx! {
        div {
            class: "absolute inset-y-0 -right-1 z-10 w-2 cursor-col-resize",
            onmousedown: move |event: Event<MouseData>| {
                event.prevent_default();
                resizing.set(true);
            },
            div { class: "mx-auto h-full w-px bg-transparent transition-colors duration-150 hover:bg-primary/40" }
        }
    }
}

struct ActiveStack;

impl ActiveStack {
    fn of(panes: &[PaneNode]) -> Option<(u64, u64)> {
        let mut fallback = None;
        for pane in panes {
            for stack in &pane.stacks {
                if !stack.is_active {
                    continue;
                }
                if pane.is_active {
                    return Some((pane.id, stack.id));
                }
                if fallback.is_none() {
                    fallback = Some((pane.id, stack.id));
                }
            }
        }
        fallback
    }
}

#[derive(Clone, Copy)]
struct StackReveal {
    settled: Signal<Option<(u64, u64)>>,
    prefix: &'static str,
}

impl StackReveal {
    fn side_sheet(settled: Signal<Option<(u64, u64)>>) -> Self {
        Self {
            settled,
            prefix: "sidesheet-stack",
        }
    }

    fn forget(mut self) {
        if (self.settled)().is_some() {
            self.settled.set(None);
        }
    }

    fn follow(mut self, target: (u64, u64)) {
        let Some(settled) = (self.settled)() else {
            self.settled.set(Some(target));
            return;
        };
        if settled == target {
            return;
        }
        let (pane_id, stack_id) = target;
        if ScrollIntoView::nearest(&format!("{}-{pane_id}-{stack_id}", self.prefix)) {
            self.settled.set(Some(target));
        }
    }
}

#[component]
fn SideSheetView(
    panes: Vec<PaneNode>,
    active_space: Option<vmux_core::event::space::SpaceRow>,
    bookmarks: BookmarksHostEvent,
    projects: Vec<vmux_core::event::ProjectRow>,
    boundary: Option<crate::event::TabBoundary>,
    team: Vec<TeamMemberRow>,
    pane_tree_error: Option<String>,
) -> Element {
    let active_pane = panes
        .iter()
        .find(|pane| pane.is_active)
        .or_else(|| panes.first())
        .cloned();
    let active_page = active_pane
        .as_ref()
        .and_then(|pane| pane.stacks.iter().find(|stack| stack.is_active))
        .filter(|stack| !stack.url.is_empty())
        .cloned();
    let folders = bookmark_folder_choices(&bookmarks.roots);
    let initial_folders = folders.clone();
    let mut folder_context = use_signal(|| initial_folders);
    let drag_state = use_signal(|| None::<BookmarkDragState>);
    let optimistic_pin_order = use_signal(|| None::<OptimisticPinOrder>);
    use_context_provider(|| folder_context);
    use_context_provider(|| drag_state);
    use_context_provider(|| optimistic_pin_order);
    use_effect(move || folder_context.set(folders.clone()));
    use_drop(move || {
        set_bookmark_context_menu_active(false);
    });
    rsx! {
        div {
            class: "flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto px-2 pb-3 pt-2 text-foreground [scrollbar-gutter:stable]",
            ..BookmarkDragState::listeners(drag_state, optimistic_pin_order),
            if let Some(space) = active_space {
                div { class: "glass mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg",
                    SideSheetSpaceRow { key: "{space.id}", space: space.clone() }
                    ActiveSessionPanel {
                        active_page: active_page.clone(),
                        team: team.clone(),
                        projects: projects.clone(),
                        boundary: boundary.clone(),
                        pane_id: active_pane.as_ref().map(|pane| pane.id).unwrap_or_default(),
                    }
                }
            }
            if let Some(pane) = active_pane {
                BookmarksSection {
                    bookmarks: bookmarks.clone(),
                    active_page,
                    pane_id: pane.id,
                    expanded: pane.bookmarks_expanded,
                }
            }
            if let Some(err) = pane_tree_error {
                div { class: "flex shrink-0 items-center px-2 py-1",
                    span { class: "text-ui text-destructive", "{err}" }
                }
            } else if panes.is_empty() {
                div { class: "flex shrink-0 items-center px-2 py-1",
                    span { class: "text-ui text-muted-foreground", {translate("layout-no-stacks")} }
                }
            } else {
                for (i, pane) in panes.iter().enumerate() {
                    PaneSection { key: "{pane.id}", pane: pane.clone(), index: i }
                }
            }
        }
    }
}

fn listener_ready(received: bool, error: &Option<String>) -> bool {
    received || error.is_some()
}

fn layout_overlay_ready(
    state: &LayoutStateEvent,
    layout_ready: bool,
    stacks_ready: bool,
    tabs_ready: bool,
    pane_tree_ready: bool,
    spaces_ready: bool,
) -> bool {
    layout_ready
        && (!state.header_visible() || (stacks_ready && tabs_ready))
        && (!state.side_sheet_open || (pane_tree_ready && spaces_ready))
}

#[component]
fn UpdateNoticeFooter(phase: UpdatePhase) -> Element {
    let (label, version) = match &phase {
        UpdatePhase::Downloading { version, .. } => {
            (translate("layout-update-downloading"), version.clone())
        }
        UpdatePhase::Installing { version } => {
            (translate("layout-update-installing"), version.clone())
        }
        UpdatePhase::Ready { version } => (translate("layout-update-ready"), version.clone()),
    };
    rsx! {
        div {
            class: "shrink-0 mx-2 mb-2 mt-2 flex flex-col gap-2 rounded-md glass px-3 py-2 text-foreground",
            div { class: "flex items-center gap-2",
                span { class: "inline-block h-2 w-2 shrink-0 rounded-full bg-success" }
                span { class: "min-w-0 flex-1 text-ui font-medium", "{label}" }
                span { class: "shrink-0 text-xs text-muted-foreground", "{version}" }
            }
            {match phase {
                UpdatePhase::Downloading { downloaded, total, .. } => rsx! {
                    UpdateProgressBar { downloaded, total }
                },
                UpdatePhase::Installing { .. } => rsx! {
                    UpdateProgressBar { downloaded: 0, total: 0 }
                },
                UpdatePhase::Ready { .. } => rsx! {
                    button {
                        r#type: "button",
                        class: "w-full cursor-pointer rounded-md bg-primary px-2.5 py-1.5 text-ui font-medium text-primary-foreground hover:opacity-90",
                        onclick: move |_| {
                            let _ = send(&crate::event::RestartRequestEvent);
                        },
                        {translate("layout-restart-update")}
                    }
                },
            }}
        }
    }
}

#[component]
fn HeaderView(
    stacks_state: StacksHostEvent,
    tabs_state: TabsHostEvent,
    bookmarks: BookmarksHostEvent,
    team: Vec<TeamMemberRow>,
    extensions: Vec<ExtRow>,
    remote: RemoteStateEvent,
    reload_key: u32,
    stacks_error: Option<String>,
    tabs_error: Option<String>,
) -> Element {
    let tab_drag = use_tab_drag();
    let StacksHostEvent {
        stacks,
        can_go_back,
        can_go_forward,
        is_zoomed: _,
    } = stacks_state;
    let TabsHostEvent { tabs } = tabs_state;
    let host_tab_order = tabs.iter().map(|tab| tab.id.clone()).collect::<Vec<_>>();
    let host_tab_activation = tabs
        .iter()
        .map(|tab| (tab.id.clone(), tab.is_active))
        .collect::<Vec<_>>();
    let tab_drag_region_revision = tabs
        .iter()
        .map(|tab| tab.id.as_str())
        .collect::<Vec<_>>()
        .join(":");
    let mut active_sync = tab_drag;
    use_effect(use_reactive!(|(host_tab_activation, host_tab_order)| {
        let host_active_tab_id = host_tab_activation
            .iter()
            .find(|(_, is_active)| *is_active)
            .map(|(id, _)| id.clone());
        active_sync.acknowledge_host(host_active_tab_id, host_tab_order);
    }));
    let tabs = tab_drag.ordered(tabs);
    let tab_metrics_style = format!("--tab-width:{TAB_WIDTH_PX}px;--tab-gap:{TAB_GAP_PX}px;");
    let active_row = stacks.iter().find(|t| t.is_active).cloned();
    let active_bg_color = active_row.as_ref().and_then(|r| r.bg_color.clone());
    let active_url = active_row
        .as_ref()
        .map(|r| r.url.clone())
        .unwrap_or_default();
    let show_bookmark = !active_url.is_empty();
    let is_bookmarked = show_bookmark
        && (bookmark_nodes_contain_url(&bookmarks.roots, &active_url)
            || bookmarks
                .pins
                .iter()
                .any(|pin| pin.metadata.url == active_url && pin.bookmarked));
    let pinned_uuid = bookmarks
        .pins
        .iter()
        .find(|pin| pin.metadata.url == active_url)
        .map(|pin| pin.uuid.clone());
    let is_pinned = pinned_uuid.is_some();
    let active_metadata = active_row.as_ref().map(|row| PageMetadata {
        title: row.title.clone(),
        url: row.url.clone(),
        icon: row.icon.clone(),
        bg_color: row.bg_color.clone(),
    });

    let (url_row_style, url_row_class) = url_row_cef(active_bg_color.as_deref());

    rsx! {
        div {
            class: "flex h-full min-h-0 min-w-0 flex-col text-foreground",
            ..tab_drag.listeners(),
            div {
                class: "flex min-w-0 shrink-0 items-center gap-1 pr-2",
                WindowDragRegion {
                    id: "leading",
                    revision: tab_drag_region_revision.clone(),
                    class: "h-10 shrink-0 self-stretch",
                    style: "margin-left:min(56px,var(--vmux-tab-row-pad-left));width:max(0px,calc(var(--vmux-tab-row-pad-left) - 56px));",
                }
                if let Some(err) = tabs_error {
                    span { class: "text-ui text-destructive", "{err}" }
                } else {
                    div {
                        class: "flex min-w-0 flex-1 items-center gap-[var(--tab-gap)] overflow-x-auto overflow-y-hidden pl-2",
                        style: "{tab_metrics_style}",
                        for (tab_index, tab) in tabs.iter().enumerate() {
                            {
                                let mut tab = tab.clone();
                                tab.is_active = tab_drag.is_active(&tab.id, tab.is_active);
                                if tab.is_active {
                                    tab.bg_color = active_bg_color.clone();
                                }
                                rsx! {
                                    Tab {
                                        key: "{tab.id}",
                                        tab,
                                        index: tab_index,
                                        drag: tab_drag,
                                    }
                                }
                            }
                        }
                        NewTabButton {}
                        WindowDragRegion {
                            id: "trailing",
                            revision: tab_drag_region_revision.clone(),
                            class: "h-10 min-w-0 flex-1 self-stretch",
                            style: "",
                        }
                    }
                }
            }
            div {
                class: "{url_row_class}",
                style: "{url_row_style}",
                if let Some(err) = stacks_error {
                    span { class: "text-ui text-destructive", "{err}" }
                } else {
                    NavButton { label: translate("layout-back"), command: "prev_page", disabled: !can_go_back,
                        Icon { class: "h-4 w-4",
                            path { d: "M19 12H5" }
                            path { d: "M12 19l-7-7 7-7" }
                        }
                    }
                    NavButton { label: translate("layout-forward"), command: "next_page", disabled: !can_go_forward,
                        Icon { class: "h-4 w-4",
                            path { d: "M5 12h14" }
                            path { d: "M12 5l7 7-7 7" }
                        }
                    }
                    NavButton { label: translate("layout-reload"), command: "reload", disabled: active_row.as_ref().is_none_or(|t| t.url.is_empty()),
                        span {
                            key: "{reload_key}",
                            class: if reload_key > 0 { "inline-flex animate-spin-once" } else { "inline-flex" },
                            Icon { class: "h-4 w-4",
                                path { d: "M21 12a9 9 0 11-3-6.7L21 8" }
                                path { d: "M21 3v5h-5" }
                            }
                        }
                    }
                    HeaderAddressBar {
                        active_row: active_row.clone(),
                        bg_color: active_bg_color.clone(),
                    }
                    if show_bookmark {
                        button {
                            r#type: "button",
                            aria_label: if is_bookmarked { translate("layout-remove-bookmark") } else { translate("layout-bookmark-page") },
                            title: if is_bookmarked { format!("{} (\u{2318}D)", translate("layout-remove-bookmark")) } else { format!("{} (\u{2318}D)", translate("layout-bookmark-page")) },
                            class: if is_bookmarked {
                                "flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-foreground transition-colors hover:bg-glass-hover"
                            } else {
                                "flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground"
                            },
                            onclick: move |_| {
                                let _ = send(&BookmarksCommandEvent {
                                    command: "toggle_active".into(),
                                    uuid: None,
                                    name: None,
                                    url: None,
                                    metadata: None,
                                    folder: None,
                                    target_uuid: None,
                                });
                            },
                            Icon { class: "h-4 w-4",
                                path {
                                    d: "M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z",
                                    fill: if is_bookmarked { "currentColor" } else { "none" },
                                }
                            }
                        }
                        button {
                            r#type: "button",
                            aria_label: if is_pinned { translate("layout-unpin-page") } else { translate("layout-pin-page") },
                            title: if is_pinned { translate("layout-unpin-page") } else { translate("layout-pin-page") },
                            class: if is_pinned {
                                "flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-foreground transition-colors hover:bg-glass-hover"
                            } else {
                                "flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground"
                            },
                            onclick: move |_| {
                                if let Some(uuid) = pinned_uuid.clone() {
                                    bookmark_cmd("unpin", Some(uuid));
                                } else if let Some(metadata) = active_metadata.clone() {
                                    add_to_bookmarks("pin_url", metadata, None);
                                }
                            },
                            Icon { class: "h-4 w-4",
                                path { d: "M12 17v5" }
                                path { d: "M5 17h14" }
                                path { d: "M6 3h12" }
                                path {
                                    d: "M8 3v5a6 6 0 0 1-2 4v1h12v-1a6 6 0 0 1-2-4V3",
                                    fill: if is_pinned { "currentColor" } else { "none" },
                                }
                            }
                        }
                    }
                    TeamFacepile { members: team }
                    ExtensionBar { extensions }
                    RemoteControl { remote }
                }
            }
        }
    }
}

fn url_row_cef(_bg_color: Option<&str>) -> (String, String) {
    (
        String::new(),
        "flex min-w-0 flex-1 shrink-0 items-center gap-1 rounded-t-[var(--radius)] px-2 bg-glass backdrop-blur-xl backdrop-saturate-150 text-foreground".to_string(),
    )
}

#[component]
fn SideSheetSpaceRow(space: vmux_core::event::space::SpaceRow) -> Element {
    let editing = use_signal(|| false);
    let draft = use_signal(|| space.name.clone());
    let menu_value = use_signal(|| space.id.clone());
    let rename_id = space.id.clone();
    let open_space_id = space.id.clone();

    rsx! {
        LayoutContextMenu {
            ContextMenuTrigger { attributes: vec![],
                div {
                    class: "group relative flex w-full cursor-pointer items-center px-2 py-1.5 text-foreground hover:bg-foreground/5",
                    button {
                        r#type: "button",
                        class: if editing() {
                            "pointer-events-none absolute inset-0 z-0 rounded outline-none"
                        } else {
                            "absolute inset-0 z-0 rounded outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary/60"
                        },
                        title: space.name.clone(),
                        aria_label: space.name.clone(),
                        disabled: editing(),
                        onclick: move |_| {
                            let _ = send(&vmux_core::event::space::SpaceCommandEvent {
                                command: "open_page".to_string(),
                                space_id: Some(open_space_id.clone()),
                                name: None,
                            });
                        },
                    },
                    div { class: "pointer-events-none relative z-10 flex min-w-0 flex-1 items-center gap-2",
                        Icon { class: "h-4 w-4 shrink-0",
                            path { d: "M3 3h7v7H3z" }
                            path { d: "M14 3h7v7h-7z" }
                            path { d: "M3 14h7v7H3z" }
                            path { d: "M14 14h7v7h-7z" }
                        }
                        EditableText {
                            value: space.name.clone(),
                            editing,
                            draft,
                            display_class: "pointer-events-auto min-w-0 flex-1 cursor-text truncate rounded px-1 py-0.5 text-left text-ui font-medium text-foreground hover:bg-foreground/[0.06]".to_string(),
                            input_class: "pointer-events-auto min-w-0 flex-1 rounded bg-background/70 px-1 py-0.5 text-ui font-medium text-foreground outline-none ring-1 ring-inset ring-primary/40".to_string(),
                            title: translate("common-rename"),
                            on_active_change: set_bookmark_text_input_active,
                            on_commit: move |name| {
                                let _ = send(&vmux_core::event::space::SpaceCommandEvent {
                                    command: "rename".to_string(),
                                    space_id: Some(rename_id.clone()),
                                    name: Some(name),
                                });
                            },
                        }
                    }
                }
            }
            ContextMenuContent { attributes: vec![],
                ContextMenuItem {
                    index: 0usize,
                    value: Into::<ReadSignal<String>>::into(menu_value),
                    on_select: move |_: String| {
                        begin_inline_rename(editing, draft, space.name.clone())
                    },
                    attributes: vec![],
                    {translate("common-rename")}
                }
            }
        }
    }
}

fn dir_truncate_class(title: &str) -> &'static str {
    if title.contains('/') {
        "truncate-start"
    } else {
        "truncate"
    }
}

#[derive(Clone, PartialEq)]
struct ActiveSessionInfo {
    page: StackNode,
    agent: Option<TeamMemberRow>,
    project: Option<ActiveWorkspaceProject>,
    boundary: Option<crate::event::TabBoundary>,
}

#[derive(Clone, PartialEq)]
struct ActiveWorkspaceProject {
    root: vmux_core::event::ProjectRow,
    children: Vec<vmux_core::event::ProjectRow>,
}

impl ActiveWorkspaceProject {
    fn of(projects: &[vmux_core::event::ProjectRow]) -> Option<Self> {
        let index = projects
            .iter()
            .position(|project| project.depth == 0 && project.is_active)?;
        let root = projects[index].clone();
        let children = projects
            .iter()
            .skip(index + 1)
            .take_while(|project| project.depth > 0)
            .cloned()
            .collect();
        Some(Self { root, children })
    }
}

impl ActiveSessionInfo {
    fn of(
        page: Option<StackNode>,
        team: &[TeamMemberRow],
        projects: &[vmux_core::event::ProjectRow],
        boundary: Option<crate::event::TabBoundary>,
    ) -> Option<Self> {
        let page = page?;
        let agent = Self::agent_for(&page.url, team);
        let project = ActiveWorkspaceProject::of(projects);
        Some(Self {
            page,
            agent,
            project,
            boundary,
        })
    }

    fn agent_for(url: &str, team: &[TeamMemberRow]) -> Option<TeamMemberRow> {
        for member in team.iter().filter(|member| !member.is_user) {
            if member.sid.is_empty() {
                continue;
            }
            let segment = format!("/{}", member.sid);
            if url.ends_with(&segment) || url.contains(&format!("{segment}/")) {
                return Some(member.clone());
            }
        }
        team.iter()
            .filter(|member| !member.is_user && !member.url.is_empty())
            .find(|member| url.trim_end_matches('/') == member.url.trim_end_matches('/'))
            .cloned()
    }
}

#[component]
fn ActiveSessionPanel(
    active_page: Option<StackNode>,
    team: Vec<TeamMemberRow>,
    projects: Vec<vmux_core::event::ProjectRow>,
    boundary: Option<crate::event::TabBoundary>,
    pane_id: u64,
) -> Element {
    let Some(session) = ActiveSessionInfo::of(active_page, &team, &projects, boundary) else {
        return rsx! {};
    };
    let ActiveSessionInfo {
        page,
        agent,
        project,
        boundary,
    } = session;
    let title = if page.title.trim().is_empty() {
        page.url.clone()
    } else {
        page.title.clone()
    };
    let status = agent.as_ref().map(|agent| {
        if agent.is_running {
            ("streaming", translate("agent-status-running"))
        } else if agent.is_done_unseen {
            ("idle", translate("agent-status-done"))
        } else {
            ("idle", translate("common-current"))
        }
    });
    rsx! {
        div { class: "border-t border-foreground/10 px-2.5 py-2.5",
            div { class: "flex min-w-0 items-center gap-2.5",
                div { class: "flex size-8 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.055]",
                    PageIconView {
                        icon: page.icon.clone(),
                        url: page.url.clone(),
                        img_class: "size-4 shrink-0 rounded-sm object-contain".to_string(),
                        icon_class: "size-4 shrink-0 text-muted-foreground".to_string(),
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate text-ui font-semibold text-foreground", title: "{title}", "{title}" }
                    div { class: "truncate text-[10px] text-muted-foreground", title: "{page.url}", "{page.url}" }
                }
            }
            div { class: "mt-2 flex flex-col gap-1",
                if let Some(agent) = agent {
                    div { class: "flex min-w-0 items-center gap-2 rounded-md bg-foreground/[0.035] px-2 py-1.5",
                        Avatar {
                            src: agent.icon.clone(),
                            seed: agent.name.clone(),
                            background: agent.color.clone(),
                            alt: agent.name.clone(),
                            class: "size-4 text-[7px]",
                        }
                        span { class: "min-w-0 flex-1 truncate text-[10px] font-medium text-foreground", "{agent.name}" }
                        if let Some((status, label)) = status {
                            span { class: "flex shrink-0 items-center gap-1.5 text-[10px] text-muted-foreground",
                                StatusDot { status: status.to_string(), size_class: "size-1.5".to_string() }
                                "{label}"
                            }
                        }
                    }
                }
                if let Some(project) = project {
                    ActiveWorkspaceProjectTree { project, pane_id }
                }
                if let Some(boundary) = boundary {
                    if boundary.is_git_repo {
                        ActiveSessionGit { boundary }
                    }
                }
            }
        }
    }
}

#[component]
fn ActiveSessionGit(boundary: crate::event::TabBoundary) -> Element {
    let repository = if boundary.repository.is_empty() {
        translate("composer-git-repository")
    } else {
        boundary.repository.clone()
    };
    let branch = if boundary.branch.is_empty() {
        repository.clone()
    } else {
        boundary.branch.clone()
    };
    let relation = if boundary.base_ref.is_empty() || boundary.base_ref == boundary.branch {
        branch
    } else {
        format!("{} → {}", boundary.base_ref, branch)
    };
    rsx! {
        div { class: "min-w-0 rounded-md bg-foreground/[0.035] px-2.5 py-2.5",
            div { class: "flex min-w-0 items-center gap-2",
                LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0 text-muted-foreground".to_string() }
                span { class: "min-w-0 flex-1 truncate text-[10px] font-semibold text-foreground", title: "{repository}", "{repository}" }
                if boundary.is_worktree {
                    span { class: "shrink-0 rounded-full bg-primary/10 px-1.5 py-0.5 text-[9px] font-medium text-primary",
                        {translate("layout-worktree")}
                    }
                }
            }
            div { class: "mt-1.5 flex min-w-0 items-center gap-2 font-mono text-[10px] text-muted-foreground",
                span { class: "min-w-0 flex-1 truncate", title: "{relation}", "{relation}" }
            }
            div { class: "mt-2 grid grid-cols-2 gap-1.5 text-[9px]",
                div { class: "flex min-w-0 items-center gap-1.5 rounded-md bg-foreground/[0.04] px-2 py-1.5 text-muted-foreground",
                    span { class: "size-1.5 shrink-0 rounded-full bg-amber-400" }
                    span { class: "min-w-0 flex-1 truncate", {translate("composer-uncommitted-changes")} }
                    span { class: "font-mono text-foreground", "{boundary.uncommitted}" }
                }
                div { class: "flex min-w-0 items-center gap-1.5 rounded-md bg-foreground/[0.04] px-2 py-1.5 text-muted-foreground",
                    LineIconView { icon: LineIcon::File, class: "size-3 shrink-0".to_string() }
                    span { class: "min-w-0 flex-1 truncate", {translate("git-status-modified")} }
                    span { class: "font-mono text-foreground", "{boundary.changed_files}" }
                }
                if boundary.ahead > 0 {
                    div { class: "flex min-w-0 items-center gap-1.5 rounded-md bg-foreground/[0.04] px-2 py-1.5 text-muted-foreground",
                        span { class: "text-sky-500", "↑" }
                        span { class: "min-w-0 flex-1 truncate", {translate("composer-commits-ahead")} }
                        span { class: "font-mono text-foreground", "{boundary.ahead}" }
                    }
                }
                div { class: "flex min-w-0 items-center justify-end gap-2 rounded-md bg-foreground/[0.04] px-2 py-1.5 font-mono",
                    span { class: "text-success", "+{boundary.insertions}" }
                    span { class: "text-destructive", "−{boundary.deletions}" }
                }
            }
            if !boundary.effective_dir.is_empty() {
                div { class: "mt-2 truncate font-mono text-[9px] text-muted-foreground/65", title: "{boundary.effective_dir}", "{boundary.effective_dir}" }
            }
        }
    }
}

#[component]
fn ActiveWorkspaceProjectTree(project: ActiveWorkspaceProject, pane_id: u64) -> Element {
    let root = project.root;
    let root_path = root.path.clone();
    rsx! {
        div { class: "min-w-0 overflow-hidden rounded-md bg-foreground/[0.035]",
            button {
                r#type: "button",
                class: "flex w-full min-w-0 cursor-pointer items-center gap-2 px-2 py-1.5 text-left text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground",
                title: "{root.display_path}",
                onclick: move |_| {
                    let _ = send(&vmux_core::event::ProjectTreeToggle {
                        path: root_path.clone(),
                        pane_id: pane_id.to_string(),
                    });
                },
                Icon {
                    class: if root.expanded { SIDEBAR_TREE_CHEVRON_OPEN } else { SIDEBAR_TREE_CHEVRON_CLOSED },
                    path { d: "m9 18 6-6-6-6" }
                }
                BuiltinIconView { icon: vmux_core::BuiltinIcon::Project, class: "size-3.5 shrink-0".to_string() }
                span { class: "min-w-0 flex-1 truncate text-[10px] font-medium text-foreground", "{root.label}" }
            }
            SidebarTreeChildren { expanded: root.expanded,
                div { class: "border-t border-foreground/[0.06] py-1",
                    for child in project.children {
                        ActiveWorkspaceProjectRow {
                            key: "{child.path}",
                            project: child,
                            pane_id,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ActiveWorkspaceProjectRow(project: vmux_core::event::ProjectRow, pane_id: u64) -> Element {
    let path = project.path.clone();
    let opens_tree = project.kind.opens_a_tree();
    rsx! {
        SidebarTreeRowGroup {
            SidebarTreeRow {
                path: project.path.clone(),
                label: project.label.clone(),
                is_dir: opens_tree,
                expanded: project.expanded,
                depth: project.depth,
                title: project.display_path.clone(),
                on_activate: move |()| {
                    if opens_tree {
                        let _ = send(&vmux_core::event::ProjectTreeToggle {
                            path: path.clone(),
                            pane_id: pane_id.to_string(),
                        });
                    } else {
                        let _ = send(&crate::event::SideSheetCommandEvent {
                            command: "open_project_path".to_string(),
                            pane_id: pane_id.to_string(),
                            stack_id: 0,
                            line: 0,
                            path: path.clone(),
                        });
                    }
                },
            }
        }
    }
}

#[component]
fn RemoteControl(remote: RemoteStateEvent) -> Element {
    let mut open = use_signal(|| false);
    use_effect(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "remote".to_string(),
            active: open(),
        });
    });
    use_drop(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "remote".to_string(),
            active: false,
        });
    });
    rsx! {
        div { class: "relative ml-1 shrink-0",
            div {
                class: if remote.enabled {
                    "flex h-7 items-center overflow-hidden rounded-full border border-success/25 bg-success/10 text-success"
                } else {
                    "flex h-7 items-center overflow-hidden rounded-full border border-foreground/10 bg-foreground/[0.04] text-muted-foreground"
                },
                button {
                    r#type: "button",
                    class: "flex h-full items-center gap-1.5 pl-2 pr-1.5 text-[10px] font-semibold hover:bg-foreground/[0.06]",
                    aria_label: "Live",
                    onclick: move |_| open.set(!open()),
                    span { class: if remote.enabled { "size-1.5 rounded-full bg-success" } else { "size-1.5 rounded-full bg-muted-foreground/50" } }
                    "Live"
                }
                button {
                    r#type: "button",
                    class: "relative mx-1 h-4 w-7 shrink-0 rounded-full bg-foreground/15",
                    aria_label: "Toggle Live",
                    aria_pressed: remote.enabled,
                    onclick: move |_| {
                        let _ = send(&RemoteCommandEvent {
                            enabled: !remote.enabled,
                        });
                    },
                    span {
                        class: if remote.enabled {
                            "absolute left-3.5 top-0.5 size-3 rounded-full bg-white shadow-sm transition-all"
                        } else {
                            "absolute left-0.5 top-0.5 size-3 rounded-full bg-foreground/70 shadow-sm transition-all"
                        }
                    }
                }
            }
            if open() {
                button {
                    r#type: "button",
                    class: "pointer-events-auto fixed inset-0 z-[998] m-0 h-screen w-screen cursor-default border-0 bg-transparent p-0 outline-none",
                    aria_label: translate("common-close"),
                    onpointerdown: move |event| {
                        event.prevent_default();
                        open.set(false);
                    },
                    oncontextmenu: move |event| {
                        event.prevent_default();
                        open.set(false);
                    },
                }
                div { class: "glass absolute right-0 top-9 z-[999] w-72 overflow-hidden rounded-xl border border-border/80 bg-background/95 shadow-2xl backdrop-blur-xl",
                    RemotePanel { remote: remote.clone() }
                }
            }
        }
    }
}

#[component]
fn RemotePanel(remote: RemoteStateEvent) -> Element {
    let mut show_pairing = use_signal(|| false);
    let mut pairing_generation = use_signal(|| 0_u64);
    let mut pairing_started_paired = use_signal(|| false);
    let mut copied = use_signal(|| false);
    let active = remote.phase == RemotePhase::Enabled;
    let transitioning = remote.phase == RemotePhase::Starting;
    let status = match remote.phase {
        RemotePhase::Disabled | RemotePhase::Enabled => None,
        RemotePhase::Starting if remote.enabled => Some("Starting…"),
        RemotePhase::Starting => Some("Stopping…"),
        RemotePhase::Error => Some("Needs attention"),
    };
    let qr = if active
        && show_pairing()
        && (!remote.paired || pairing_started_paired())
        && !remote.pairing_deep_link.is_empty()
    {
        pairing_qr_svg(&remote.pairing_deep_link)
    } else {
        None
    };
    rsx! {
        div { class: "p-3",
            div { class: "flex items-center gap-2",
                div {
                    class: if remote.enabled {
                        "flex size-7 shrink-0 items-center justify-center rounded-md bg-success/15 text-success"
                    } else {
                        "flex size-7 shrink-0 items-center justify-center rounded-md bg-foreground/5 text-muted-foreground"
                    },
                    Icon { class: "size-4",
                        path { d: "M12 2a10 10 0 1 0 10 10" }
                        path { d: "M12 12 22 2" }
                        path { d: "M15 2h7v7" }
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "text-ui font-semibold", "Live" }
                    if let Some(status) = status {
                        div {
                            class: if remote.phase == RemotePhase::Error {
                                "mt-0.5 truncate text-[10px] text-destructive"
                            } else {
                                "mt-0.5 text-[10px] text-muted-foreground"
                            },
                            "{status}"
                        }
                    }
                }
                button {
                    r#type: "button",
                    class: if remote.enabled {
                        "relative h-5 w-9 shrink-0 rounded-full bg-success transition-colors"
                    } else {
                        "relative h-5 w-9 shrink-0 rounded-full bg-foreground/15 transition-colors"
                    },
                    aria_label: "Toggle Live",
                    aria_pressed: remote.enabled,
                    onclick: move |_| {
                        if remote.enabled {
                            pairing_generation.set(pairing_generation().wrapping_add(1));
                            show_pairing.set(false);
                        }
                        let _ = send(&RemoteCommandEvent {
                            enabled: !remote.enabled,
                        });
                    },
                    span {
                        class: if remote.enabled {
                            "absolute left-[18px] top-0.5 size-4 rounded-full bg-white shadow-sm transition-all"
                        } else {
                            "absolute left-0.5 top-0.5 size-4 rounded-full bg-white shadow-sm transition-all"
                        }
                    }
                }
            }
            if remote.phase == RemotePhase::Error {
                div { class: "mt-2 rounded-md border border-destructive/20 bg-destructive/5 p-2",
                    div { class: "break-words text-[10px] leading-4 text-destructive", "{remote.error}" }
                    button {
                        r#type: "button",
                        class: "mt-1.5 text-[10px] font-semibold text-foreground hover:opacity-70",
                        onclick: move |_| {
                            let _ = send(&RemoteCommandEvent {
                                enabled: remote.enabled,
                            });
                        },
                        "Retry"
                    }
                }
            } else if transitioning {
                div { class: "mt-2 h-1 overflow-hidden rounded-full bg-foreground/10",
                    div { class: "h-full w-full rounded-full bg-success" }
                }
            } else if active {
                if let Some(svg) = qr {
                    div { class: "mt-2 flex items-center justify-between gap-2",
                        div { class: "text-[10px] font-semibold text-foreground", "Connect a device" }
                        button {
                            r#type: "button",
                            class: "rounded px-1.5 py-1 text-[9px] font-semibold text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                            onclick: move |_| {
                                pairing_generation.set(pairing_generation().wrapping_add(1));
                                show_pairing.set(false);
                            },
                            "Close"
                        }
                    }
                    div { class: "mt-2 flex flex-col items-center rounded-lg bg-white p-2.5 text-zinc-950",
                        div {
                            class: "w-full rounded-sm [&>svg]:block [&>svg]:aspect-square [&>svg]:h-auto [&>svg]:w-full",
                            dangerous_inner_html: "{svg}",
                        }
                        div { class: "mt-1.5 text-center text-[10px] font-semibold", "Scan with your phone" }
                        div { class: "mt-0.5 text-center text-[9px] text-zinc-500", "Opens Vmux and pairs automatically" }
                    }
                    div { class: "mt-2 flex items-center gap-1.5 rounded-md bg-foreground/5 py-1 pl-2 pr-1",
                        div {
                            class: "min-w-0 flex-1 truncate font-mono text-[9px] text-muted-foreground",
                            title: "{remote.pairing_url}",
                            "{remote.pairing_url}"
                        }
                        button {
                            r#type: "button",
                            class: "shrink-0 rounded px-1.5 py-1 text-[9px] font-semibold text-foreground hover:bg-foreground/10",
                            onclick: move |_| {
                                let _ = send(&RemoteCopyEvent);
                                copied.set(true);
                            },
                            if copied() { "Copied" } else { "Copy" }
                        }
                    }
                    div { class: "mt-1.5 text-[9px] leading-4 text-muted-foreground",
                        "Pairing details hide automatically after 2 minutes."
                    }
                } else {
                    div { class: "mt-2 flex items-center gap-2",
                        div { class: if remote.paired { "flex min-w-0 flex-1 items-center gap-1.5 text-[10px] text-success" } else { "flex min-w-0 flex-1 items-center gap-1.5 text-[10px] text-muted-foreground" },
                            span { class: if remote.paired { "size-1.5 rounded-full bg-success" } else { "size-1.5 rounded-full bg-foreground/25" } }
                            if remote.paired { "Phone paired" } else { "No phone paired" }
                        }
                        button {
                            r#type: "button",
                            class: "text-[10px] font-semibold text-foreground hover:opacity-70",
                            onclick: move |_| {
                                copied.set(false);
                                pairing_started_paired.set(remote.paired);
                                let generation = pairing_generation().wrapping_add(1);
                                pairing_generation.set(generation);
                                show_pairing.set(true);
                                spawn(async move {
                                    sleep_ms(120_000).await;
                                    if pairing_generation() == generation {
                                        show_pairing.set(false);
                                    }
                                });
                            },
                            "Connect device"
                        }
                    }
                }
            }
        }
    }
}

fn pairing_qr_svg(value: &str) -> Option<String> {
    use qrcode::QrCode;
    use qrcode::render::svg;

    let code = QrCode::new(value).ok()?;
    Some(
        code.render::<svg::Color>()
            .min_dimensions(148, 148)
            .dark_color(svg::Color("#09090b"))
            .light_color(svg::Color("#ffffff"))
            .build(),
    )
}

struct PageUrl;

impl PageUrl {
    fn matches(left: &str, right: &str) -> bool {
        left.trim_end_matches('/') == right.trim_end_matches('/')
    }
}

#[component]
fn BookmarksSection(
    bookmarks: BookmarksHostEvent,
    active_page: Option<StackNode>,
    pane_id: u64,
    expanded: bool,
) -> Element {
    let BookmarksHostEvent { pins, roots } = bookmarks;
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let mut optimistic_pin_order: Signal<Option<OptimisticPinOrder>> = use_context();
    let mut creating_folder = use_signal(|| false);
    let new_folder_draft = use_signal(|| translate("layout-new-folder"));
    let bookmark_menu_action: Signal<BookmarkMenuActionEvent> = use_context();
    let initial_menu_action = bookmark_menu_action.peek().sequence;
    let mut handled_menu_action = use_signal(|| initial_menu_action);
    use_effect(move || {
        let action = bookmark_menu_action();
        if action.sequence == handled_menu_action() {
            return;
        }
        handled_menu_action.set(action.sequence);
        if action.action == "new_folder" && action.uuid.is_none() {
            begin_new_folder(creating_folder, new_folder_draft);
        }
    });
    let folders = bookmark_folder_choices(&roots);
    let folder_rows = bookmark_folder_rows(&roots);
    let host_pin_order = pins.iter().map(|pin| pin.uuid.clone()).collect::<Vec<_>>();
    let observed_host_pin_order = host_pin_order.clone();
    use_effect(use_reactive!(|observed_host_pin_order| {
        let Some(optimistic) = optimistic_pin_order() else {
            return;
        };
        if observed_host_pin_order == optimistic.expected
            || observed_host_pin_order != optimistic.baseline
        {
            optimistic_pin_order.set(None);
        }
    }));
    let pins = optimistic_pin_order()
        .as_ref()
        .map(|optimistic| optimistic.apply(&pins))
        .unwrap_or(pins);
    let active_url = active_page.as_ref().map(|page| page.url.clone());
    let rendered_pin_order = Rc::new(pins.iter().map(|pin| pin.uuid.clone()).collect::<Vec<_>>());
    let root_targeted = bookmark_drop_targeted(drag_state, &BookmarkDropTarget::Root);
    let root_drop_label = drag_state()
        .filter(|drag| drag.active)
        .map(|drag| match drag.item {
            BookmarkDragItem::Page { .. } => translate("layout-add-to-bookmarks"),
            BookmarkDragItem::Bookmark { .. }
            | BookmarkDragItem::Pin { .. }
            | BookmarkDragItem::Folder { .. } => translate("layout-move-to-bookmarks"),
        });
    let bookmarks_title = translate("layout-bookmarks");
    let new_folder_title = translate("layout-new-folder");

    rsx! {
        div {
            "data-bookmark-drop": "root",
            class: "glass group relative z-30 mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg",
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                request_bookmark_menu("menu_root", None, None);
            },
            div {
                "data-bookmark-drop": "root",
                onpointerenter: move |_| set_bookmark_drop_target(drag_state, BookmarkDropTarget::Root),
                onpointerleave: move |_| clear_bookmark_drop_target(drag_state, &BookmarkDropTarget::Root),
                class: if root_targeted {
                    "flex items-center bg-foreground/10 ring-1 ring-inset ring-ring"
                } else {
                    "flex items-center transition-colors hover:bg-glass-hover"
                },
                div { class: "flex min-w-0 flex-1 items-center gap-2 px-2.5 py-2",
                    div { class: "grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-foreground/[0.07] text-foreground ring-1 ring-inset ring-foreground/10",
                        Icon { class: "h-3.5 w-3.5",
                            path { d: "M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" }
                        }
                    }
                    if let Some(label) = root_drop_label {
                        span { class: "min-w-0 flex-1 text-ui font-semibold text-foreground", "{label}" }
                    } else {
                        span { class: "min-w-0 flex-1 text-ui font-semibold text-foreground", "{bookmarks_title}" }
                        button {
                            r#type: "button",
                            aria_label: "{new_folder_title}",
                            title: "{new_folder_title}",
                            class: "flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                            onclick: move |event| {
                                event.prevent_default();
                                event.stop_propagation();
                                begin_new_folder(creating_folder, new_folder_draft);
                            },
                            Icon { class: "h-3.5 w-3.5 pointer-events-none",
                                path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                path { d: "M12 10v6" }
                                path { d: "M9 13h6" }
                            }
                        }
                    }
                }
                button {
                    r#type: "button",
                    aria_label: "{bookmarks_title}",
                    title: "{bookmarks_title}",
                    class: if expanded {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10 hover:text-foreground"
                    } else {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm bg-foreground/10 text-foreground"
                    },
                    onclick: move |_| set_side_sheet_section(pane_id, "bookmarks", !expanded),
                    Icon {
                        class: if expanded { SIDEBAR_CARD_CHEVRON_OPEN } else { SIDEBAR_CARD_CHEVRON_CLOSED },
                        path { d: "m9 18 6-6-6-6" }
                    }
                }
            }
            div { class: if expanded {
                    "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                } else {
                    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                },
                div { class: "overflow-hidden",
                    div { class: "border-t border-foreground/10 p-1.5",
                        if !pins.is_empty() {
                            div {
                                "data-bookmark-drop": "",
                                class: "mb-1 grid grid-cols-4 gap-1.5 p-1",
                                for (index, pin) in pins.iter().enumerate() {
                                    PinTile {
                                        key: "{pin.uuid}",
                                        row: pin.clone(),
                                        index,
                                        pin_order: rendered_pin_order.clone(),
                                        active: active_url.as_ref().is_some_and(|active_url| PageUrl::matches(active_url, &pin.metadata.url)),
                                    }
                                }
                            }
                        }
                        if creating_folder() {
                            div { class: "flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                                Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                                    path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                }
                                InlineEdit {
                                    draft: new_folder_draft,
                                    class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none".to_string(),
                                    placeholder: translate("layout-folder-name"),
                                    aria_label: translate("layout-folder-name"),
                                    on_active_change: set_bookmark_text_input_active,
                                    on_commit: move |name| {
                                        creating_folder.set(false);
                                        create_bookmark_folder(name, None);
                                    },
                                    on_cancel: move |_| creating_folder.set(false),
                                }
                            }
                        }
                        if pins.is_empty() && roots.is_empty() && !creating_folder() {
                            div { class: "px-2 py-2 text-ui-xs text-muted-foreground", {translate("layout-no-pins-bookmarks")} }
                        } else {
                            div { class: SIDEBAR_TREE_SCROLLER,
                            div { class: "{SIDEBAR_TREE_COLUMN} gap-1",
                                for node in roots.iter() {
                                    match node {
                                        BookmarkNode::Folder(f) if f.parent.is_none() => rsx! {
                                            BookmarkFolder {
                                                key: "{f.uuid}",
                                                folder: f.clone(),
                                                parent_uuid: None,
                                                folders: folders.clone(),
                                                folder_rows: folder_rows.clone(),
                                                active_page: active_page.clone(),
                                            }
                                        },
                                        BookmarkNode::Folder(_) => rsx! {},
                                        BookmarkNode::Entry(b) => rsx! {
                                            BookmarkEntry {
                                                key: "{b.uuid}",
                                                row: b.clone(),
                                                folder_uuid: None,
                                                folders: folders.clone(),
                                            }
                                        },
                                    }
                                }
                            }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn set_side_sheet_section(pane_id: u64, section: &str, expanded: bool) {
    let _ = send(&crate::event::SideSheetCommandEvent {
        command: if expanded {
            "expand_section".to_string()
        } else {
            "collapse_section".to_string()
        },
        pane_id: pane_id.to_string(),
        stack_id: 0,
        line: 0,
        path: section.to_string(),
    });
}

#[component]
fn PaneSection(pane: PaneNode, index: usize) -> Element {
    let label = translate_with(
        "layout-stack-number",
        &[("number", TranslationValue::Number((index + 1) as i64))],
    );
    let pane_id = pane.id;
    let any_loading = pane.stacks.iter().any(|s| s.is_loading);
    let expanded = !pane.collapsed;
    let fold_title = if expanded {
        translate("layout-fold-stack")
    } else {
        translate("layout-unfold-stack")
    };
    let visible_stacks = pane
        .stacks
        .iter()
        .filter(|stack| !(stack.url.is_empty() && stack.title == "New Stack"))
        .cloned()
        .collect::<Vec<_>>();
    let active_stack = visible_stacks
        .iter()
        .find(|stack| stack.is_active)
        .or_else(|| visible_stacks.first())
        .cloned();
    rsx! {
        div { class: if pane.is_active && any_loading {
                "glass group mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg pane-loading-ring"
            } else {
                "glass group mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg"
            },
            div {
                class: "flex items-center transition-colors hover:bg-glass-hover",
                div { class: "flex min-w-0 flex-1 items-center gap-2 px-2.5 py-2",
                    div { class: if pane.is_active {
                            "grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary ring-1 ring-inset ring-primary/20"
                        } else {
                            "grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-foreground/[0.07] text-foreground ring-1 ring-inset ring-foreground/10"
                        },
                        Icon { class: "h-3.5 w-3.5",
                            path { d: "M4 6h16M4 12h16M4 18h16" }
                        }
                    }
                    span {
                        class: if pane.is_active {
                            "min-w-0 flex-1 text-ui font-semibold text-foreground"
                        } else {
                            "min-w-0 flex-1 text-ui font-medium text-muted-foreground"
                        },
                        "{label}"
                    }
                }
                button {
                    r#type: "button",
                    aria_label: "{fold_title}",
                    title: "{fold_title}",
                    class: if expanded {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10 hover:text-foreground"
                    } else {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm bg-foreground/10 text-foreground"
                    },
                    onclick: move |_| set_side_sheet_section(pane_id, "pane", !expanded),
                    Icon {
                        class: if expanded { SIDEBAR_CARD_CHEVRON_OPEN } else { SIDEBAR_CARD_CHEVRON_CLOSED },
                        path { d: "m9 18 6-6-6-6" }
                    }
                }
            }
            div { class: "border-t border-foreground/10 p-1.5",
                div { class: "flex flex-col gap-1",
                    if !expanded && let Some(stack) = active_stack {
                        SideSheetStackRow { stack, pane_id }
                    }
                    div { class: if expanded {
                            "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                        } else {
                            "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                        },
                        div { class: "min-h-0 overflow-hidden",
                            div { class: "flex flex-col gap-1",
                                for stack in visible_stacks.iter() {
                                    SideSheetStackRow { stack: stack.clone(), pane_id }
                                }
                                NewStackRow { pane_id }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct BookmarkFolderChoice {
    uuid: String,
    label: String,
    ancestors: Vec<String>,
}

#[derive(Clone, PartialEq)]
enum BookmarkDragItem {
    Page {
        metadata: PageMetadata,
    },
    Bookmark {
        uuid: String,
    },
    Pin {
        uuid: String,
        source_index: usize,
        order: Rc<Vec<String>>,
    },
    Folder {
        uuid: String,
    },
}

#[derive(Clone, PartialEq)]
enum BookmarkDropTarget {
    Root,
    Folder(String),
    Pin { uuid: String, index: usize },
}

#[derive(Clone, PartialEq)]
struct BookmarkDragState {
    item: BookmarkDragItem,
    start_x: f64,
    start_y: f64,
    current_x: f64,
    current_y: f64,
    active: bool,
    target: Option<BookmarkDropTarget>,
}

#[derive(Clone, PartialEq)]
struct OptimisticPinOrder {
    baseline: Vec<String>,
    expected: Vec<String>,
}

impl OptimisticPinOrder {
    fn after_drop(order: &[String], source_index: usize, target_index: usize) -> Option<Self> {
        if source_index == target_index
            || source_index >= order.len()
            || target_index >= order.len()
        {
            return None;
        }
        let mut expected = order.to_vec();
        let moved = expected.remove(source_index);
        expected.insert(target_index, moved);
        Some(Self {
            baseline: order.to_vec(),
            expected,
        })
    }

    fn apply(&self, pins: &[BookmarkRow]) -> Vec<BookmarkRow> {
        let mut ordered = Vec::with_capacity(pins.len());
        for uuid in &self.expected {
            if let Some(pin) = pins.iter().find(|pin| &pin.uuid == uuid) {
                ordered.push(pin.clone());
            }
        }
        for pin in pins {
            if !self.expected.contains(&pin.uuid) {
                ordered.push(pin.clone());
            }
        }
        ordered
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct PinDragVisual {
    offset_x: f64,
    offset_y: f64,
    source: bool,
    active: bool,
    dragging: bool,
}

impl PinDragVisual {
    fn of(state: Option<&BookmarkDragState>, uuid: &str, index: usize) -> Self {
        let Some(state) = state.filter(|state| state.active) else {
            return Self::default();
        };
        let BookmarkDragItem::Pin {
            uuid: source_uuid,
            source_index,
            ..
        } = &state.item
        else {
            return Self::default();
        };
        let target_index = match state.target.as_ref() {
            Some(BookmarkDropTarget::Pin { index, .. }) => Some(*index),
            _ => None,
        };
        if source_uuid == uuid {
            return Self {
                offset_x: state.current_x - state.start_x,
                offset_y: state.current_y - state.start_y,
                source: true,
                active: true,
                dragging: true,
            };
        }
        let Some(target_index) = target_index else {
            return Self {
                dragging: true,
                ..Self::default()
            };
        };
        let destination =
            if *source_index < target_index && index > *source_index && index <= target_index {
                index - 1
            } else if target_index < *source_index && index >= target_index && index < *source_index
            {
                index + 1
            } else {
                index
            };
        let column_delta = destination as isize % 4 - index as isize % 4;
        let row_delta = destination as isize / 4 - index as isize / 4;
        Self {
            offset_x: column_delta as f64,
            offset_y: row_delta as f64,
            source: false,
            active: destination != index,
            dragging: true,
        }
    }

    fn style(self) -> String {
        if !self.dragging {
            return "transform:none;z-index:auto;pointer-events:auto;transition:transform 140ms ease;"
                .to_string();
        }
        if !self.active {
            return "transform:none;z-index:auto;pointer-events:none;transition:transform 140ms ease;"
                .to_string();
        }
        if self.source {
            return format!(
                "transform:translate3d({}px,{}px,0);z-index:20;pointer-events:none;transition:none;opacity:.9;",
                self.offset_x, self.offset_y
            );
        }
        format!(
            "transform:translate3d({}, {}, 0);z-index:auto;pointer-events:none;transition:transform 140ms ease;",
            pin_grid_offset(self.offset_x as isize),
            pin_grid_offset(self.offset_y as isize)
        )
    }
}

fn pin_grid_offset(delta: isize) -> String {
    match delta.cmp(&0) {
        std::cmp::Ordering::Equal => "0px".to_string(),
        std::cmp::Ordering::Greater => {
            format!("calc({}% + {}rem)", delta * 100, delta as f64 * 0.375)
        }
        std::cmp::Ordering::Less => format!(
            "calc({}% - {}rem)",
            delta * 100,
            delta.unsigned_abs() as f64 * 0.375
        ),
    }
}

impl BookmarkDragState {
    fn listeners(
        state: Signal<Option<Self>>,
        optimistic_pin_order: Signal<Option<OptimisticPinOrder>>,
    ) -> Vec<Attribute> {
        if state.read().is_none() {
            return Vec::new();
        }

        vec![
            dioxus_elements::events::onpointermove(move |event| {
                update_bookmark_drag(state, &event)
            }),
            dioxus_elements::events::onpointerup(move |event| {
                end_bookmark_drag(state, optimistic_pin_order, &event)
            }),
            dioxus_elements::events::onpointercancel(move |event| {
                cancel_bookmark_drag(state, &event)
            }),
        ]
    }
}

fn bookmark_nodes_contain_url(nodes: &[BookmarkNode], url: &str) -> bool {
    nodes.iter().any(|node| match node {
        BookmarkNode::Entry(bookmark) => bookmark.metadata.url == url,
        BookmarkNode::Folder(folder) => folder
            .children
            .iter()
            .any(|bookmark| bookmark.metadata.url == url),
    })
}

fn bookmark_folder_rows(nodes: &[BookmarkNode]) -> Vec<FolderRow> {
    nodes
        .iter()
        .filter_map(|node| match node {
            BookmarkNode::Folder(folder) => Some(folder.clone()),
            BookmarkNode::Entry(_) => None,
        })
        .collect()
}

fn bookmark_folder_choices(nodes: &[BookmarkNode]) -> Vec<BookmarkFolderChoice> {
    fn collect(
        folders: &[FolderRow],
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
            let label = if parent_label.is_empty() {
                folder.name.clone()
            } else {
                format!("{parent_label} / {}", folder.name)
            };
            output.push(BookmarkFolderChoice {
                uuid: folder.uuid.clone(),
                label: label.clone(),
                ancestors: ancestors.to_vec(),
            });
            let mut child_ancestors = ancestors.to_vec();
            child_ancestors.push(folder.uuid.clone());
            collect(
                folders,
                Some(&folder.uuid),
                &label,
                &child_ancestors,
                visited,
                output,
            );
        }
    }

    let folders = bookmark_folder_rows(nodes);
    let mut output = Vec::new();
    collect(
        &folders,
        None,
        "",
        &[],
        &mut std::collections::HashSet::new(),
        &mut output,
    );
    output
}

#[component]
fn UpdateProgressBar(downloaded: u64, total: u64) -> Element {
    rsx! {
        Progress {
            value: (total > 0).then(|| download_pct(downloaded, total) as f64),
            attributes: vec![],
            ProgressIndicator { attributes: vec![] }
        }
    }
}

#[derive(Clone, PartialEq)]
struct TabDragState {
    source_id: String,
    source_index: usize,
    target_index: usize,
    order: Vec<String>,
    start_x: f64,
    start_y: f64,
    current_x: f64,
    active: bool,
}

#[derive(Clone, PartialEq)]
struct TabClickBlock {
    source_id: String,
    target_id: String,
}

#[derive(Clone, Copy, Default, PartialEq)]
struct TabDragVisual {
    offset_x: f64,
    source: bool,
    active: bool,
}

const TAB_WIDTH_PX: f64 = 208.0;
const TAB_GAP_PX: f64 = 4.0;
const TAB_STEP_PX: f64 = TAB_WIDTH_PX + TAB_GAP_PX;

impl TabDragState {
    fn update_target(&mut self) {
        let slots = ((self.current_x - self.start_x) / TAB_STEP_PX).round() as isize;
        let last = self.order.len().saturating_sub(1) as isize;
        self.target_index = (self.source_index as isize + slots).clamp(0, last) as usize;
    }

    fn source_offset(&self) -> f64 {
        let min = -(self.source_index as f64) * TAB_STEP_PX;
        let max = self.order.len().saturating_sub(self.source_index + 1) as f64 * TAB_STEP_PX;
        (self.current_x - self.start_x).clamp(min, max)
    }

    fn offset_for(&self, index: usize) -> f64 {
        if index == self.source_index {
            return self.source_offset();
        }
        if self.source_index < self.target_index
            && index > self.source_index
            && index <= self.target_index
        {
            return -TAB_STEP_PX;
        }
        if self.target_index < self.source_index
            && index >= self.target_index
            && index < self.source_index
        {
            return TAB_STEP_PX;
        }
        0.0
    }
}

impl TabDragVisual {
    fn of(state: Option<&TabDragState>, tab_id: &str, index: usize) -> Self {
        let Some(state) = state.filter(|state| state.active) else {
            return Self::default();
        };
        Self {
            offset_x: state.offset_for(index),
            source: state.source_id == tab_id,
            active: true,
        }
    }

    fn style(self) -> String {
        if !self.active || (!self.source && self.offset_x.abs() < f64::EPSILON) {
            return "transform:none;z-index:auto;pointer-events:auto;transition:transform 140ms ease;"
                .to_string();
        }
        if self.source {
            return format!(
                "transform:translate3d({}px,0,0);z-index:20;pointer-events:none;transition:none;",
                self.offset_x
            );
        }
        format!(
            "transform:translate3d({}px,0,0);z-index:auto;pointer-events:auto;transition:transform 140ms ease;",
            self.offset_x
        )
    }
}

#[derive(Clone, Copy, PartialEq)]
struct TabDrag {
    state: Signal<Option<Rc<TabDragState>>>,
    click_block: Signal<Option<TabClickBlock>>,
    host_active: Signal<Option<String>>,
    host_order: Signal<Vec<String>>,
    optimistic_order: Signal<Option<Vec<String>>>,
    optimistic_active: Signal<Option<String>>,
}

fn use_tab_drag() -> TabDrag {
    TabDrag {
        state: use_signal(|| None::<Rc<TabDragState>>),
        click_block: use_signal(|| None),
        host_active: use_signal(|| None),
        host_order: use_signal(Vec::new),
        optimistic_order: use_signal(|| None),
        optimistic_active: use_signal(|| None),
    }
}

impl TabDrag {
    fn listeners(self) -> Vec<Attribute> {
        let mut advancing = self;
        let mut finishing = self;
        let mut cancelling = self;
        let mut leaving = self;
        vec![
            dioxus_elements::events::onpointermove(move |event| advancing.advance(&event)),
            dioxus_elements::events::onpointerup(move |event| finishing.finish(&event)),
            dioxus_elements::events::onpointercancel(move |_| cancelling.cancel()),
            dioxus_elements::events::onpointerleave(move |_| leaving.cancel()),
        ]
    }

    fn begin(
        &mut self,
        event: &Event<PointerData>,
        source_id: String,
        source_index: usize,
        draggable: bool,
    ) {
        if !draggable || event.trigger_button() != Some(MouseButton::Primary) {
            return;
        }
        event.prevent_default();
        self.click_block.set(None);
        let point = event.client_coordinates();
        let order = (self.optimistic_order)().unwrap_or_else(|| (self.host_order)());
        let source_index = order
            .iter()
            .position(|id| id == &source_id)
            .unwrap_or(source_index);
        self.state.set(Some(Rc::new(TabDragState {
            source_id,
            source_index,
            target_index: source_index,
            order,
            start_x: point.x,
            start_y: point.y,
            current_x: point.x,
            active: false,
        })));
    }

    fn advance(&mut self, event: &Event<PointerData>) {
        let Some(mut state) = (self.state)().as_deref().cloned() else {
            return;
        };
        let point = event.client_coordinates();
        let dx = point.x - state.start_x;
        let dy = point.y - state.start_y;
        state.current_x = point.x;
        state.update_target();
        if !state.active && dx * dx + dy * dy < 16.0 {
            return;
        }
        if !state.active {
            state.active = true;
        }
        self.state.set(Some(Rc::new(state)));
    }

    fn finish(&mut self, event: &Event<PointerData>) {
        let Some(state) = (self.state)() else {
            return;
        };
        if !state.active {
            self.state.set(None);
            return;
        }
        event.prevent_default();
        event.stop_propagation();
        let target_id = state
            .order
            .get(state.target_index)
            .cloned()
            .unwrap_or_else(|| state.source_id.clone());
        if state.source_index != state.target_index {
            let _ = send(&TabsCommandEvent {
                command: "reorder".to_string(),
                tab_id: Some(state.source_id.clone()),
                target_tab_id: Some(target_id.clone()),
                drop_placement: Some(if state.target_index < state.source_index {
                    TabDropPlacement::Before
                } else {
                    TabDropPlacement::After
                }),
            });
            let mut order = state.order.clone();
            if let Some(source_index) = order.iter().position(|id| id == &state.source_id)
                && state.target_index < order.len()
            {
                let moved = order.remove(source_index);
                order.insert(state.target_index, moved);
                self.optimistic_order.set(Some(order.clone()));
                let mut optimistic_order = self.optimistic_order;
                spawn(async move {
                    sleep_ms(500).await;
                    if optimistic_order() == Some(order) {
                        optimistic_order.set(None);
                    }
                });
            }
        }
        let block = TabClickBlock {
            source_id: state.source_id.clone(),
            target_id,
        };
        self.click_block.set(Some(block.clone()));
        self.state.set(None);
        let mut click_block = self.click_block;
        spawn(async move {
            sleep_ms(100).await;
            if click_block() == Some(block) {
                click_block.set(None);
            }
        });
    }

    fn cancel(&mut self) {
        self.state.set(None);
    }

    fn blocks_click(&mut self, tab_id: &str) -> bool {
        let Some(block) = (self.click_block)() else {
            return false;
        };
        if block.source_id != tab_id && block.target_id != tab_id {
            return false;
        }
        self.click_block.set(None);
        true
    }

    fn visual(self, tab_id: &str, index: usize) -> TabDragVisual {
        let state = (self.state)();
        TabDragVisual::of(state.as_deref(), tab_id, index)
    }

    fn activate(&mut self, tab_id: String) {
        if (self.host_active)().as_deref() != Some(tab_id.as_str()) {
            self.optimistic_active.set(Some(tab_id.clone()));
            let expected = tab_id.clone();
            let mut optimistic_active = self.optimistic_active;
            spawn(async move {
                sleep_ms(500).await;
                if optimistic_active().as_deref() == Some(expected.as_str()) {
                    optimistic_active.set(None);
                }
            });
        }
        let _ = send(&TabsCommandEvent {
            command: "switch".to_string(),
            tab_id: Some(tab_id),
            target_tab_id: None,
            drop_placement: None,
        });
    }

    fn acknowledge_host(&mut self, host_active_tab_id: Option<String>, host_order: Vec<String>) {
        let had_optimistic_active = self.optimistic_active.peek().is_some();
        self.host_active.set(host_active_tab_id);
        if (self.optimistic_order)().as_ref() == Some(&host_order) {
            self.optimistic_order.set(None);
        }
        self.host_order.set(host_order);
        if had_optimistic_active {
            self.optimistic_active.set(None);
        }
    }

    fn ordered(self, tabs: Vec<TabRow>) -> Vec<TabRow> {
        let Some(order) = (self.optimistic_order)() else {
            return tabs;
        };
        let mut remaining = tabs;
        let mut ordered = Vec::with_capacity(remaining.len());
        for id in order {
            let Some(index) = remaining.iter().position(|tab| tab.id == id) else {
                continue;
            };
            ordered.push(remaining.remove(index));
        }
        ordered.extend(remaining);
        ordered
    }

    fn is_active(self, tab_id: &str, host_active: bool) -> bool {
        match (self.optimistic_active)() {
            Some(active_id) => active_id == tab_id,
            None => host_active,
        }
    }
}

#[component]
fn Tab(tab: TabRow, index: usize, drag: TabDrag) -> Element {
    let visual = drag.visual(&tab.id, index);
    let id_switch = tab.id.clone();
    let id_close = tab.id.clone();
    let display_title = if !tab.title.is_empty() {
        tab.title.clone()
    } else if !tab.name.is_empty() {
        tab.name.clone()
    } else {
        translate("layout-tab")
    };
    let tooltip = display_title.clone();
    let is_active = tab.is_active;
    let skirt_classes = "relative \
        before:content-[''] before:absolute before:bottom-0 before:-left-2 before:h-2 before:w-2 before:pointer-events-none \
        before:[background:radial-gradient(circle_at_top_left,transparent_0,transparent_8px,var(--tab-bg)_8px)] \
        after:content-[''] after:absolute after:bottom-0 after:-right-2 after:h-2 after:w-2 after:pointer-events-none \
        after:[background:radial-gradient(circle_at_top_right,transparent_0,transparent_8px,var(--tab-bg)_8px)]";
    let cursor_classes = if is_active {
        "cursor-grab active:cursor-grabbing"
    } else {
        "cursor-pointer"
    };
    let tab_box_classes = cn([
        "group relative flex h-10 w-[var(--tab-width)] min-w-[var(--tab-width)] max-w-[var(--tab-width)] basis-[var(--tab-width)] shrink-0 grow-0 select-none items-start",
        cursor_classes,
    ]);
    let inactive_hover_classes = if visual.active {
        ""
    } else {
        "hover:bg-glass-hover hover:text-foreground"
    };

    let trunc = dir_truncate_class(&display_title);
    let (surface_style, surface_class, title_class, close_class) = if is_active {
        (
            "--tab-bg:var(--glass);background-color:var(--glass);border-bottom-width:0;"
                .to_string(),
            cn([
                skirt_classes,
                "glass mt-1 flex h-10 w-full items-center gap-2 rounded-t-md border-b-0 px-3.5 transition-[height,margin,background-color,border-color,border-radius,color,box-shadow] duration-150 ease-out",
            ]),
            cn([
                "min-w-0 flex-1",
                trunc,
                "text-ui font-medium text-foreground transition-colors duration-150 ease-out",
            ]),
            "flex h-4 w-4 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10".to_string(),
        )
    } else {
        (
            "background-color:color-mix(in oklab,var(--glass) 58%,transparent);".to_string(),
            cn([
                "my-1 flex h-8 w-full items-center gap-2 rounded-lg border border-glass-border/65 px-3.5 text-muted-foreground shadow-sm transition-[height,margin,background-color,border-color,border-radius,color,box-shadow] duration-150 ease-out",
                inactive_hover_classes,
            ]),
            cn([
                "min-w-0 flex-1",
                trunc,
                "text-ui transition-colors duration-150 ease-out",
            ]),
            "flex h-4 w-4 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10".to_string(),
        )
    };
    let tab_style = visual.style();

    let bookmark_metadata = PageMetadata {
        title: display_title.clone(),
        url: tab.url.clone(),
        icon: tab.icon.clone(),
        bg_color: tab.bg_color.clone(),
    };
    let pin_metadata = bookmark_metadata.clone();
    let menu_val = use_signal(|| tab.id.clone());

    rsx! {
        LayoutContextMenu {
            ContextMenuTrigger { attributes: vec![],
        div {
            class: "{tab_box_classes}",
            style: "{tab_style}",
            onpointerdown: {
                let source_id = tab.id.clone();
                move |event| {
                    drag.begin(
                        &event,
                        source_id.clone(),
                        index,
                        is_active,
                    )
                }
            },
            onclick: move |event| {
                if drag.blocks_click(&id_switch) {
                    event.prevent_default();
                    event.stop_propagation();
                    return;
                }
                drag.activate(id_switch.clone());
            },
            WindowDragRegion {
                id: format!("tab-{}", tab.id),
                revision: index.to_string(),
                blocked: true,
                class: "pointer-events-none absolute inset-0",
                style: "",
            }
            div { class: "{surface_class}", style: "{surface_style}",
                div {
                    title: "{tooltip}",
                    class: "flex min-w-0 flex-1 items-center gap-2.5 overflow-hidden",
                    HeaderTabIcon {
                        icon: tab.icon.clone(),
                        url: tab.url.clone(),
                        title: display_title.clone(),
                    }
                    span { class: "{title_class}", "{display_title}" }
                }
                if tab.is_done_unseen {
                    span { class: "size-2 shrink-0 rounded-full bg-amber-400 ring-2 ring-background" }
                }
                button {
                    r#type: "button",
                    aria_label: translate("layout-close-tab"),
                    title: translate("layout-close-tab"),
                    class: "{close_class}",
                    onpointerdown: move |evt| {
                        evt.prevent_default();
                        evt.stop_propagation();
                    },
                    onmousedown: move |evt| {
                        evt.prevent_default();
                        evt.stop_propagation();
                    },
                    onclick: move |evt| {
                        evt.prevent_default();
                        evt.stop_propagation();
                        let _ = send(&TabsCommandEvent {
                            command: "close".to_string(),
                            tab_id: Some(id_close.clone()),
                            target_tab_id: None,
                            drop_placement: None,
                        });
                    },
                    Icon { class: "h-2.5 w-2.5",
                        path { d: "M18 6 6 18" }
                        path { d: "m6 6 12 12" }
                    }
                }
            }
        }
            }
            ContextMenuContent { attributes: vec![],
                ContextMenuItem {
                    index: 0usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| add_to_bookmarks("add", bookmark_metadata.clone(), None),
                    attributes: vec![],
                    {translate("layout-bookmark")}
                }
                ContextMenuItem {
                    index: 1usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| add_to_bookmarks("pin_url", pin_metadata.clone(), None),
                    attributes: vec![],
                    {translate("layout-pin")}
                }
            }
        }
    }
}

#[component]
fn HeaderTabIcon(icon: PageIcon, url: String, title: String) -> Element {
    if title == "New Stack" && url.is_empty() {
        return rsx! { StackIcon { icon, url, title } };
    }
    let initial_ready = !icon.is_none();
    let mut displayed_icon = use_signal(|| icon.clone());
    let mut displayed_url = use_signal(|| url.clone());
    let mut fallback_ready = use_signal(|| initial_ready);
    let mut generation = use_signal(|| 0_u32);
    use_effect(use_reactive!(|(icon, url)| {
        let next = generation.peek().wrapping_add(1);
        generation.set(next);
        if !icon.is_none() {
            displayed_icon.set(icon);
            displayed_url.set(url);
            fallback_ready.set(true);
            return;
        }
        fallback_ready.set(false);
        let url = url.clone();
        spawn(async move {
            sleep_ms(500).await;
            if generation() != next {
                return;
            }
            displayed_icon.set(PageIcon::None);
            displayed_url.set(url);
            fallback_ready.set(true);
        });
    }));
    let shown_icon = displayed_icon();
    if shown_icon.is_none() && !fallback_ready() {
        return rsx! { span { class: "h-4 w-4 shrink-0" } };
    }
    rsx! {
        StackIcon {
            icon: shown_icon,
            url: displayed_url(),
            title,
        }
    }
}

#[component]
fn NewTabButton() -> Element {
    rsx! {
        div { class: "relative h-7 w-7 shrink-0",
            WindowDragRegion {
                id: "new-tab",
                revision: String::new(),
                blocked: true,
                class: "pointer-events-none absolute inset-0",
                style: "",
            }
            button {
                r#type: "button",
                aria_label: translate("layout-new-tab"),
                title: translate("layout-new-tab"),
                class: "absolute inset-0 flex cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground active:bg-glass-active active:text-foreground",
                onclick: move |_| {
                    let _ = send(&TabsCommandEvent {
                        command: "new".to_string(),
                        tab_id: None,
                        target_tab_id: None,
                        drop_placement: None,
                    });
                },
                Icon { class: "h-3.5 w-3.5",
                    path { d: "M12 5v14" }
                    path { d: "M5 12h14" }
                }
            }
        }
    }
}

#[component]
fn WindowDragRegion(
    #[props(into)] id: String,
    revision: String,
    #[props(default)] blocked: bool,
    class: &'static str,
    style: &'static str,
) -> Element {
    let reporter = WindowDragReporter {
        id,
        blocked,
        region: use_signal(|| None::<Rc<MountedData>>),
    };
    let dropped = reporter.clone();
    use_drop(move || dropped.remove());
    let revised = reporter.clone();
    use_effect(use_reactive!(|revision| {
        let _ = revision;
        revised.publish();
    }));
    let mounted = reporter.clone();
    let resized = reporter;

    rsx! {
        div {
            class,
            style,
            onmounted: move |event: Event<MountedData>| {
                mounted.clone().mount(event.data());
            },
            onresize: move |_: Event<ResizeData>| resized.publish(),
        }
    }
}

#[derive(Clone)]
struct WindowDragReporter {
    id: String,
    blocked: bool,
    region: Signal<Option<Rc<MountedData>>>,
}

impl WindowDragReporter {
    fn mount(mut self, region: Rc<MountedData>) {
        self.region.set(Some(region));
        self.publish();
    }

    fn publish(&self) {
        let region = self.region;
        let id = self.id.clone();
        let blocked = self.blocked;
        spawn(async move {
            let Some(region) = region() else {
                return;
            };
            let Ok(rect) = region.get_client_rect().await else {
                return;
            };
            let _ = send(&WindowDragRegionEvent {
                id,
                removed: false,
                blocked,
                left: rect.origin.x as f32,
                top: rect.origin.y as f32,
                width: rect.size.width as f32,
                height: rect.size.height as f32,
            });
        });
    }

    fn remove(&self) {
        let _ = send(&WindowDragRegionEvent {
            id: self.id.clone(),
            removed: true,
            ..Default::default()
        });
    }
}

#[component]
fn NavButton(
    label: String,
    command: &'static str,
    #[props(default)] disabled: bool,
    children: Element,
) -> Element {
    let class = if disabled {
        "flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground/40 transition-colors cursor-default"
    } else {
        "cursor-pointer flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground active:bg-glass-active active:text-foreground"
    };
    rsx! {
        button {
            r#type: "button",
            aria_label: "{label}",
            title: "{label}",
            disabled,
            class,
            onclick: move |_| {
                if !disabled {
                    let _ = send(&HeaderCommandEvent {
                        header_command: command.to_string(),
                    });
                }
            },
            {children}
        }
    }
}

#[component]
fn HeaderAddressBar(active_row: Option<StackRow>, bg_color: Option<String>) -> Element {
    let has_content = active_row.as_ref().is_some_and(|row| !row.url.is_empty());
    let address = active_row.map(|row| row.address).unwrap_or_default();
    let empty_class = if bg_color.is_some() {
        "opacity-50"
    } else {
        "text-muted-foreground"
    };

    rsx! {
        div {
            class: "flex h-8 min-w-0 flex-1 cursor-pointer items-center gap-2",
            onclick: move |_| {
                let _ = send(&HeaderCommandEvent {
                    header_command: "focus_address_bar".to_string(),
                });
            },
            if !has_content {
                span { class: "truncate text-ui {empty_class}", {translate("layout-new-stack")} }
            } else {
                if !address.origin.is_empty() {
                    span {
                        class: "shrink-0 rounded-full bg-foreground/10 px-2 py-0.5 text-ui leading-tight",
                        "{address.origin}"
                    }
                }
                span { class: "min-w-0 truncate text-ui", "{address.rest}" }
            }
        }
    }
}

#[component]
fn TeamFacepile(members: Vec<TeamMemberRow>) -> Element {
    if members.is_empty() {
        return rsx! {};
    }
    let user = members.iter().find(|m| m.is_user).cloned();
    let agents: Vec<TeamMemberRow> = members.iter().filter(|m| !m.is_user).cloned().collect();
    let max = 5usize;
    let overflow = agents.len().saturating_sub(max);
    rsx! {
        div {
            class: "flex shrink-0 items-center gap-2 pl-3 pr-3",
            if let Some(user) = user {
                div {
                    class: "flex items-center gap-1.5 rounded-full bg-foreground/10 py-0.5 pl-0.5 pr-2.5 cursor-pointer transition-opacity hover:opacity-80",
                    title: translate("team-profiles"),
                    onclick: move |_| {
                        let _ = send(&TeamCommandEvent {
                            command: "open".to_string(),
                            member_id: None,
                            profile_id: None,
                            profile_name: None,
                        });
                    },
                    Avatar {
                        src: None,
                        seed: user.name.clone(),
                        background: user.color.clone(),
                        alt: user.name.clone(),
                        class: "size-5 text-[9px]",
                    }
                    span { class: "whitespace-nowrap text-xs font-medium text-foreground", "{user.name}" }
                }
            }
            if !agents.is_empty() {
                div { class: "flex items-center -space-x-1.5",
                    for m in agents.iter().take(max) {
                        {
                            let src = favicon_src_for_url(&m.icon, &m.url);
                            let id = m.id.clone();
                            rsx! {
                                div {
                                    key: "{m.id}",
                                    title: "{m.name}",
                                    class: "relative inline-flex size-5 shrink-0 cursor-pointer transition-opacity hover:opacity-80",
                                    onclick: move |_| {
                                        let _ = send(&TeamCommandEvent {
                                            command: "focus".to_string(),
                                            member_id: Some(id.clone()),
                                            profile_id: None,
                                            profile_name: None,
                                        });
                                    },
                                    Avatar {
                                        src,
                                        seed: m.name.clone(),
                                        background: m.color.clone(),
                                        alt: m.name.clone(),
                                        class: "size-5 text-[9px] ring-2 ring-background",
                                    }
                                    if m.is_running {
                                        span { class: "absolute -bottom-0.5 -right-0.5 size-1.5 rounded-full bg-success ring-2 ring-background" }
                                    } else if m.is_done_unseen {
                                        span { class: "absolute -bottom-0.5 -right-0.5 size-2 rounded-full bg-amber-400 ring-2 ring-background" }
                                    }
                                }
                            }
                        }
                    }
                    if overflow > 0 {
                        div {
                            class: "relative inline-flex size-5 items-center justify-center rounded-full ring-2 ring-background bg-muted text-[9px] font-medium text-muted-foreground cursor-pointer transition-opacity hover:opacity-80",
                            title: translate("team-profiles"),
                            onclick: move |_| {
                                let _ = send(&TeamCommandEvent {
                                    command: "open".to_string(),
                                    member_id: None,
                                    profile_id: None,
                                    profile_name: None,
                                });
                            },
                            "+{overflow}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ExtensionBar(extensions: Vec<ExtRow>) -> Element {
    let open = use_signal(|| false);
    let mut trigger = use_signal(|| None::<Rc<MountedData>>);
    let menu = ExtensionMenuState { open, trigger };
    let enabled = extensions
        .into_iter()
        .filter(|extension| extension.enabled)
        .map(Rc::new)
        .collect::<Vec<_>>();
    use_effect(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "extensions".to_string(),
            active: open(),
        });
    });
    use_drop(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "extensions".to_string(),
            active: false,
        });
    });

    rsx! {
        div { class: "relative flex shrink-0 items-center gap-1 pl-1",
            for ext in enabled.iter().filter(|extension| extension.pinned) {
                ExtensionActionButton { key: "{ext.id}", extension: ext.clone() }
            }
            button {
                r#type: "button",
                class: "flex h-7 w-7 items-center justify-center rounded-lg text-foreground/80 hover:bg-foreground/[0.08]",
                title: translate("layout-manage-extensions"),
                aria_label: translate("extensions-title"),
                aria_expanded: open(),
                onmounted: move |event| trigger.set(Some(event.data())),
                onclick: move |_| menu.toggle(),
                Icon { class: "h-4 w-4",
                    path { d: "M20.5 11H19V7c0-1.1-.9-2-2-2h-4V3.5C13 2.12 11.88 1 10.5 1S8 2.12 8 3.5V5H4c-1.1 0-1.99.9-1.99 2v3.8H3.5c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7H2V20c0 1.1.9 2 2 2h3.8v-1.5c0-1.49 1.21-2.7 2.7-2.7 1.49 0 2.7 1.21 2.7 2.7V22H17c1.1 0 2-.9 2-2v-4h1.5c1.38 0 2.5-1.12 2.5-2.5S21.88 11 20.5 11z" }
                }
            }
            if open() {
                div {
                    class: "pointer-events-auto fixed inset-0 z-[998] h-screen w-screen bg-transparent",
                    aria_hidden: "true",
                    onpointerdown: move |event| {
                        event.prevent_default();
                        menu.close();
                    },
                    oncontextmenu: move |event| {
                        event.prevent_default();
                        menu.close();
                    },
                }
                div {
                    class: "glass absolute right-0 top-9 z-[999] w-72 overflow-hidden rounded-xl border border-border/80 bg-background/95 shadow-2xl backdrop-blur-xl outline-none",
                    tabindex: "-1",
                    onmounted: move |event: Event<MountedData>| {
                        let target = event.data();
                        spawn(async move {
                            let _ = target.set_focus(true).await;
                        });
                    },
                    onkeydown: move |event| {
                        if event.key() == Key::Escape {
                            event.prevent_default();
                            menu.close();
                        }
                    },
                    div { class: "border-b border-border/70 px-3 py-2 text-xs font-semibold text-foreground",
                        {translate("extensions-title")}
                    }
                    if enabled.is_empty() {
                        div { class: "px-3 py-4 text-center text-xs text-muted-foreground",
                            {translate("extensions-empty")}
                        }
                    } else {
                        div { class: "max-h-72 overflow-y-auto p-1.5",
                            for extension in enabled.iter() {
                                ExtensionMenuRow {
                                    extension: extension.clone(),
                                    anchor: trigger,
                                    on_close: move |_| menu.close(),
                                }
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "flex w-full items-center gap-2 border-t border-border/70 px-3 py-2 text-left text-xs font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground",
                        onclick: move |_| {
                            menu.close();
                            let _ = send(&ExtOpenManagerRequest);
                        },
                        Icon { class: "size-3.5",
                            path { d: "M12 15.5A3.5 3.5 0 1 0 12 8a3.5 3.5 0 0 0 0 7.5Z" }
                            path { d: "M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06A1.7 1.7 0 0 0 15 19.4a1.7 1.7 0 0 0-1 .6 1.7 1.7 0 0 0-.4 1.1V21a2 2 0 1 1-4 0v-.09A1.7 1.7 0 0 0 8.6 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-.6-1 1.7 1.7 0 0 0-1.1-.4H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.6 8.6a1.7 1.7 0 0 0-.34-1.88l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-.6 1.7 1.7 0 0 0 .4-1.1V3a2 2 0 1 1 4 0v.09A1.7 1.7 0 0 0 15.4 4.6a1.7 1.7 0 0 0 1.88-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 19.4 9c.12.37.34.7.65.94.3.24.68.37 1.06.37H21a2 2 0 1 1 0 4h-.09c-.38 0-.76.13-1.06.37-.31.24-.53.57-.65.94Z" }
                        }
                        {translate("layout-manage-extensions")}
                    }
                }
            }
        }
    }
}

#[component]
fn ExtensionActionButton(extension: Rc<ExtRow>) -> Element {
    let mounted = use_signal(|| None::<Rc<MountedData>>);
    let action = ExtensionActionState {
        id: extension.id.clone(),
        mounted,
    };

    rsx! {
        button {
            r#type: "button",
            class: "flex h-7 w-7 items-center justify-center rounded-lg hover:bg-foreground/[0.08]",
            title: "{extension.name}",
            aria_label: "{extension.name}",
            onmounted: move |event: Event<MountedData>| {
                let mut mounted = mounted;
                mounted.set(Some(event.data()));
            },
            onclick: move |_| action.clone().open(),
            if let Some(icon) = extension.icon.as_ref() {
                img { class: "h-4 w-4", src: "{icon}", alt: "" }
            } else {
                Icon { class: "h-4 w-4",
                    path { d: "M20.5 11H19V7c0-1.1-.9-2-2-2h-4V3.5C13 2.12 11.88 1 10.5 1S8 2.12 8 3.5V5H4c-1.1 0-1.99.9-1.99 2v3.8H3.5c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7H2V20c0 1.1.9 2 2 2h3.8v-1.5c0-1.49 1.21-2.7 2.7-2.7 1.49 0 2.7 1.21 2.7 2.7V22H17c1.1 0 2-.9 2-2v-4h1.5c1.38 0 2.5-1.12 2.5-2.5S21.88 11 20.5 11z" }
                }
            }
        }
    }
}

#[derive(Clone)]
struct ExtensionActionState {
    id: String,
    mounted: Signal<Option<Rc<MountedData>>>,
}

impl ExtensionActionState {
    fn open(self) {
        spawn(async move {
            let anchor = match self.mounted.peek().clone() {
                Some(mounted) => mounted
                    .get_client_rect()
                    .await
                    .ok()
                    .map(|rect| ExtensionPopupAnchor {
                        right: (rect.origin.x + rect.size.width) as i32,
                        bottom: (rect.origin.y + rect.size.height) as i32,
                    })
                    .unwrap_or_default(),
                None => ExtensionPopupAnchor::default(),
            };
            let _ = send(&ExtActionRequest {
                id: self.id,
                anchor,
            });
        });
    }
}

#[derive(Clone, Copy)]
struct ExtensionMenuState {
    open: Signal<bool>,
    trigger: Signal<Option<Rc<MountedData>>>,
}

impl ExtensionMenuState {
    fn toggle(mut self) {
        if (self.open)() {
            self.close();
        } else {
            self.open.set(true);
        }
    }

    fn close(mut self) {
        self.open.set(false);
        let Some(trigger) = self.trigger.peek().clone() else {
            return;
        };
        spawn(async move {
            let _ = trigger.set_focus(true).await;
        });
    }
}

#[component]
fn ExtensionMenuRow(
    extension: Rc<ExtRow>,
    anchor: Signal<Option<Rc<MountedData>>>,
    on_close: EventHandler<()>,
) -> Element {
    let action_id = extension.id.clone();
    let pin_id = extension.id.clone();
    let pinned = extension.pinned;

    rsx! {
        div { class: "group flex min-w-0 items-center gap-1 rounded-lg hover:bg-foreground/[0.06]",
            button {
                r#type: "button",
                class: "flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left",
                title: "{extension.name}",
                onclick: move |_| {
                    on_close.call(());
                    ExtensionActionState {
                        id: action_id.clone(),
                        mounted: anchor,
                    }
                    .open();
                },
                if let Some(icon) = extension.icon.as_ref() {
                    img { class: "size-4 shrink-0 rounded object-contain", src: "{icon}", alt: "" }
                } else {
                    Icon { class: "size-4 shrink-0 text-muted-foreground",
                        path { d: "M20.5 11H19V7c0-1.1-.9-2-2-2h-4V3.5C13 2.12 11.88 1 10.5 1S8 2.12 8 3.5V5H4c-1.1 0-1.99.9-1.99 2v3.8H3.5c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7H2V20c0 1.1.9 2 2 2h3.8v-1.5c0-1.49 1.21-2.7 2.7-2.7 1.49 0 2.7 1.21 2.7 2.7V22H17c1.1 0 2-.9 2-2v-4h1.5c1.38 0 2.5-1.12 2.5-2.5S21.88 11 20.5 11z" }
                    }
                }
                span { class: "min-w-0 flex-1 truncate text-xs font-medium text-foreground", "{extension.name}" }
            }
            button {
                r#type: "button",
                class: if pinned {
                    "flex size-7 shrink-0 items-center justify-center rounded-md text-primary hover:bg-primary/10"
                } else {
                    "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-60 hover:bg-foreground/[0.08] hover:text-foreground group-hover:opacity-100"
                },
                title: if pinned { translate("layout-unpin") } else { translate("layout-pin") },
                aria_label: if pinned { translate("layout-unpin") } else { translate("layout-pin") },
                onclick: move |event| {
                    event.stop_propagation();
                    let _ = send(&ExtPinRequest {
                        id: pin_id.clone(),
                        pinned: !pinned,
                    });
                },
                Icon { class: "size-3.5",
                    path { d: "M12 17v5" }
                    path { d: "M5 17h14" }
                    path { d: "M6 3h12" }
                    path {
                        d: "M8 3v5a6 6 0 0 1-2 4v1h12v-1a6 6 0 0 1-2-4V3",
                        fill: if pinned { "currentColor" } else { "none" },
                    }
                }
            }
        }
    }
}

#[component]
fn PinTile(row: BookmarkRow, index: usize, pin_order: Rc<Vec<String>>, active: bool) -> Element {
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let url_open = row.metadata.url.clone();
    let uuid_unpin = row.uuid.clone();
    let menu_val = use_signal(|| row.uuid.clone());
    let drop_target = BookmarkDropTarget::Pin {
        uuid: row.uuid.clone(),
        index,
    };
    let leave_target = drop_target.clone();
    let targeted = bookmark_drop_targeted(drag_state, &drop_target);
    let visual = PinDragVisual::of(drag_state().as_ref(), &row.uuid, index);
    let drag_item = BookmarkDragItem::Pin {
        uuid: row.uuid.clone(),
        source_index: index,
        order: pin_order,
    };
    rsx! {
        div {
            class: "relative aspect-square",
            onpointerenter: move |_| set_bookmark_drop_target(drag_state, drop_target.clone()),
            onpointerleave: move |_| clear_bookmark_drop_target(drag_state, &leave_target),
            if targeted {
                div { class: "pointer-events-none absolute inset-0 z-10 rounded-md ring-2 ring-primary/70" }
            }
            BookmarkContextMenu {
                command: "menu_pin".to_string(),
                uuid: Some(row.uuid.clone()),
                metadata: None,
                trigger: rsx! {
                    div {
                        "data-bookmark-drag-source": "true",
                        onpointerdown: {
                            let item = drag_item.clone();
                            move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                        },
                        style: "{visual.style()}",
                        class: if visual.dragging {
                            "absolute inset-0 flex cursor-grabbing select-none items-center justify-center rounded-md bg-white/5"
                        } else if active {
                            "absolute inset-0 flex cursor-pointer select-none items-center justify-center rounded-md bg-primary/15 text-primary ring-1 ring-inset ring-primary/30 transition-colors hover:bg-primary/20"
                        } else {
                            "absolute inset-0 flex cursor-pointer select-none items-center justify-center rounded-md bg-white/5 transition-colors hover:bg-white/10"
                        },
                        onclick: {
                            let u = url_open.clone();
                            move |event| {
                                if bookmark_drag_blocks_click(drag_state) {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    return;
                                }
                                open_bookmark(u.clone());
                            }
                        },
                        title: "{row.metadata.title}",
                        if row.metadata.url.starts_with("vmux://") {
                            Favicon {
                                favicon_url: row.metadata.icon.favicon_url().to_string(),
                                url: row.metadata.url.clone(),
                                class: "pointer-events-none h-5 w-5 shrink-0 rounded-sm object-contain".to_string(),
                                globe_class: "pointer-events-none h-5 w-5 shrink-0 text-muted-foreground".to_string(),
                            }
                        } else {
                            PageIconView {
                                icon: row.metadata.icon.clone(),
                                url: row.metadata.url.clone(),
                                img_class: "pointer-events-none h-5 w-5 shrink-0 rounded-sm object-contain".to_string(),
                                icon_class: "pointer-events-none h-5 w-5 shrink-0 text-muted-foreground".to_string(),
                            }
                        }
                    }
                },
                menu: rsx! {
                    SideSheetContextMenuContent {
                        ContextMenuItem {
                            index: 0usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let u = url_open.clone(); move |_: String| open_bookmark(u.clone()) },
                            attributes: vec![],
                            {translate("common-open")}
                        }
                        ContextMenuItem {
                            index: 1usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let id = uuid_unpin.clone(); move |_: String| bookmark_cmd("unpin", Some(id.clone())) },
                            attributes: vec![],
                            {translate("layout-unpin-page")}
                        }
                        if row.bookmarked {
                            ContextMenuItem {
                                index: 2usize,
                                value: Into::<ReadSignal<String>>::into(menu_val),
                                on_select: { let id = row.uuid.clone(); move |_: String| bookmark_cmd("remove", Some(id.clone())) },
                                attributes: vec![],
                                {translate("layout-remove-bookmark")}
                            }
                        }
                    }
                },
            }
        }
    }
}

fn open_bookmark(url: String) {
    let _ = send(&BookmarksCommandEvent {
        command: "open".into(),
        url: Some(url),
        uuid: None,
        name: None,
        metadata: None,
        folder: None,
        target_uuid: None,
    });
}

fn bookmark_cmd(command: &str, uuid: Option<String>) {
    let _ = send(&BookmarksCommandEvent {
        command: command.into(),
        uuid,
        name: None,
        url: None,
        metadata: None,
        folder: None,
        target_uuid: None,
    });
}

fn add_to_bookmarks(command: &str, metadata: PageMetadata, folder: Option<String>) {
    let _ = send(&BookmarksCommandEvent {
        command: command.into(),
        uuid: None,
        name: None,
        url: None,
        metadata: Some(metadata),
        folder,
        target_uuid: None,
    });
}

fn move_bookmark(uuid: String, folder: Option<String>) {
    let _ = send(&BookmarksCommandEvent {
        command: "move".into(),
        uuid: Some(uuid),
        name: None,
        url: None,
        metadata: None,
        folder,
        target_uuid: None,
    });
}

fn move_pin(uuid: String, folder: Option<String>) {
    let _ = send(&BookmarksCommandEvent {
        command: "move_pin".into(),
        uuid: Some(uuid),
        name: None,
        url: None,
        metadata: None,
        folder,
        target_uuid: None,
    });
}

fn reorder_pin(uuid: String, target_uuid: String) -> bool {
    send(&BookmarksCommandEvent {
        command: "reorder_pin".into(),
        uuid: Some(uuid),
        name: None,
        url: None,
        metadata: None,
        folder: None,
        target_uuid: Some(target_uuid),
    })
    .is_ok()
}

fn move_bookmark_folder(uuid: String, folder: Option<String>) {
    let _ = send(&BookmarksCommandEvent {
        command: "move_folder".into(),
        uuid: Some(uuid),
        name: None,
        url: None,
        metadata: None,
        folder,
        target_uuid: None,
    });
}

fn commit_bookmark_rename(uuid: String, name: String) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let _ = send(&BookmarksCommandEvent {
        command: "rename".into(),
        uuid: Some(uuid),
        name: Some(name),
        url: None,
        metadata: None,
        folder: None,
        target_uuid: None,
    });
}

fn create_bookmark_folder(name: String, parent: Option<String>) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let _ = send(&BookmarksCommandEvent {
        command: "new_folder".into(),
        uuid: None,
        name: Some(name),
        url: None,
        metadata: None,
        folder: parent,
        target_uuid: None,
    });
}

fn begin_bookmark_drag(
    mut state: Signal<Option<BookmarkDragState>>,
    event: &Event<PointerData>,
    item: BookmarkDragItem,
) {
    if event.trigger_button() != Some(MouseButton::Primary) {
        return;
    }
    event.prevent_default();
    let coordinates = event.client_coordinates();
    let target = match &item {
        BookmarkDragItem::Pin {
            uuid, source_index, ..
        } => Some(BookmarkDropTarget::Pin {
            uuid: uuid.clone(),
            index: *source_index,
        }),
        _ => None,
    };
    state.set(Some(BookmarkDragState {
        item,
        start_x: coordinates.x,
        start_y: coordinates.y,
        current_x: coordinates.x,
        current_y: coordinates.y,
        active: false,
        target,
    }));
}

fn update_bookmark_drag(mut state: Signal<Option<BookmarkDragState>>, event: &Event<PointerData>) {
    let Some(mut drag) = state() else {
        return;
    };
    let coordinates = event.client_coordinates();
    let dx = coordinates.x - drag.start_x;
    let dy = coordinates.y - drag.start_y;
    if !drag.active && dx * dx + dy * dy < 16.0 {
        return;
    }
    if !drag.active {
        set_bookmark_context_menu_active(true);
    }
    drag.current_x = coordinates.x;
    drag.current_y = coordinates.y;
    drag.active = true;
    state.set(Some(drag));
}

fn perform_bookmark_drop(
    item: BookmarkDragItem,
    target: BookmarkDropTarget,
    mut optimistic_pin_order: Signal<Option<OptimisticPinOrder>>,
) {
    match (item, target) {
        (
            BookmarkDragItem::Pin {
                uuid,
                source_index,
                order,
            },
            BookmarkDropTarget::Pin {
                uuid: target_uuid,
                index: target_index,
            },
        ) => {
            if uuid != target_uuid {
                let optimistic = OptimisticPinOrder::after_drop(&order, source_index, target_index);
                if !reorder_pin(uuid, target_uuid) {
                    optimistic_pin_order.set(None);
                    return;
                }
                optimistic_pin_order.set(optimistic.clone());
                spawn(async move {
                    sleep_ms(1_000).await;
                    if optimistic_pin_order() == optimistic {
                        optimistic_pin_order.set(None);
                    }
                });
            }
        }
        (BookmarkDragItem::Page { metadata }, BookmarkDropTarget::Root) => {
            add_to_bookmarks("add", metadata, None)
        }
        (BookmarkDragItem::Page { metadata }, BookmarkDropTarget::Folder(folder)) => {
            add_to_bookmarks("add", metadata, Some(folder))
        }
        (BookmarkDragItem::Bookmark { uuid }, BookmarkDropTarget::Root) => {
            move_bookmark(uuid, None)
        }
        (BookmarkDragItem::Bookmark { uuid }, BookmarkDropTarget::Folder(folder)) => {
            move_bookmark(uuid, Some(folder))
        }
        (BookmarkDragItem::Pin { uuid, .. }, BookmarkDropTarget::Root) => move_pin(uuid, None),
        (BookmarkDragItem::Pin { uuid, .. }, BookmarkDropTarget::Folder(folder)) => {
            move_pin(uuid, Some(folder))
        }
        (BookmarkDragItem::Folder { uuid }, BookmarkDropTarget::Root) => {
            move_bookmark_folder(uuid, None)
        }
        (BookmarkDragItem::Folder { uuid }, BookmarkDropTarget::Folder(folder)) => {
            if uuid != folder {
                move_bookmark_folder(uuid, Some(folder));
            }
        }
        (BookmarkDragItem::Page { .. }, BookmarkDropTarget::Pin { .. })
        | (BookmarkDragItem::Bookmark { .. }, BookmarkDropTarget::Pin { .. })
        | (BookmarkDragItem::Folder { .. }, BookmarkDropTarget::Pin { .. }) => {}
    }
}

fn set_root_radius_px(_radius: f32) {}

fn clear_bookmark_drag_after_click(mut state: Signal<Option<BookmarkDragState>>) {
    spawn(async move {
        sleep_ms(0).await;
        state.set(None);
    });
}

fn end_bookmark_drag(
    mut state: Signal<Option<BookmarkDragState>>,
    optimistic_pin_order: Signal<Option<OptimisticPinOrder>>,
    event: &Event<PointerData>,
) {
    let Some(mut drag) = state() else {
        return;
    };
    let coordinates = event.client_coordinates();
    let dx = coordinates.x - drag.start_x;
    let dy = coordinates.y - drag.start_y;
    if !drag.active && dx * dx + dy * dy < 16.0 {
        state.set(None);
        return;
    }
    event.prevent_default();
    event.stop_propagation();
    drag.active = true;
    set_bookmark_context_menu_active(false);
    if let Some(target) = drag.target.clone() {
        perform_bookmark_drop(drag.item.clone(), target, optimistic_pin_order);
    }
    state.set(Some(drag));
    clear_bookmark_drag_after_click(state);
}

fn cancel_bookmark_drag(mut state: Signal<Option<BookmarkDragState>>, event: &Event<PointerData>) {
    event.prevent_default();
    set_bookmark_context_menu_active(false);
    state.set(None);
}

fn set_bookmark_drop_target(
    mut state: Signal<Option<BookmarkDragState>>,
    target: BookmarkDropTarget,
) {
    let Some(mut drag) = state() else {
        return;
    };
    if drag.target.as_ref() == Some(&target) {
        return;
    }
    drag.target = Some(target);
    state.set(Some(drag));
}

fn clear_bookmark_drop_target(
    mut state: Signal<Option<BookmarkDragState>>,
    target: &BookmarkDropTarget,
) {
    let Some(mut drag) = state() else {
        return;
    };
    if drag.target.as_ref() != Some(target) {
        return;
    }
    drag.target = None;
    state.set(Some(drag));
}

fn bookmark_drag_blocks_click(state: Signal<Option<BookmarkDragState>>) -> bool {
    state().is_some_and(|drag| drag.active)
}

fn bookmark_drop_targeted(
    state: Signal<Option<BookmarkDragState>>,
    target: &BookmarkDropTarget,
) -> bool {
    state().is_some_and(|drag| drag.active && drag.target.as_ref() == Some(target))
}

fn set_bookmark_text_input_active(active: bool) {
    let _ = send(&BookmarkTextInputEvent { active });
}

fn set_bookmark_context_menu_active(active: bool) {
    let _ = send(&BookmarkContextMenuEvent { active });
}

fn begin_inline_rename(mut editing: Signal<bool>, mut draft: Signal<String>, name: String) {
    draft.set(name);
    spawn(async move {
        sleep_ms(0).await;
        editing.set(true);
    });
}

#[component]
fn BookmarkFolder(
    folder: FolderRow,
    parent_uuid: Option<String>,
    folders: Vec<BookmarkFolderChoice>,
    folder_rows: Vec<FolderRow>,
    active_page: Option<StackNode>,
) -> Element {
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let uuid = folder.uuid.clone();
    let collapsed = folder.collapsed;
    let mut editing = use_signal(|| false);
    let draft = use_signal(|| folder.name.clone());
    let mut creating_child = use_signal(|| false);
    let child_draft = use_signal(|| translate("layout-new-folder"));
    let menu_val = use_signal(|| folder.uuid.clone());
    let new_folder_uuid = uuid.clone();
    let bookmark_menu_action: Signal<BookmarkMenuActionEvent> = use_context();
    let initial_menu_action = bookmark_menu_action.peek().sequence;
    let mut handled_menu_action = use_signal(|| initial_menu_action);
    let menu_action_uuid = uuid.clone();
    let menu_action_name = folder.name.clone();
    use_effect(move || {
        let action = bookmark_menu_action();
        if action.sequence == handled_menu_action() {
            return;
        }
        handled_menu_action.set(action.sequence);
        if action.uuid.as_deref() != Some(menu_action_uuid.as_str()) {
            return;
        }
        match action.action.as_str() {
            "new_folder" => begin_new_folder(creating_child, child_draft),
            "rename" => begin_inline_rename(editing, draft, menu_action_name.clone()),
            _ => {}
        }
    });
    let mut move_targets = Vec::new();
    if parent_uuid.is_some() {
        move_targets.push((None, translate("layout-move-to-bookmarks")));
    }
    move_targets.extend(
        folders
            .iter()
            .filter(|target| target.uuid != folder.uuid && !target.ancestors.contains(&folder.uuid))
            .map(|target| {
                (
                    Some(target.uuid.clone()),
                    translate_with(
                        "layout-move-to",
                        &[("folder", TranslationValue::String(&target.label))],
                    ),
                )
            }),
    );
    let remove_index = 4 + move_targets.len();
    let drop_target = BookmarkDropTarget::Folder(uuid.clone());
    let leave_target = drop_target.clone();
    let folder_targeted = bookmark_drop_targeted(drag_state, &drop_target);
    let drag_item = BookmarkDragItem::Folder { uuid: uuid.clone() };
    let mut child_folders = 0usize;
    for child in folder_rows.iter() {
        if child.parent.as_deref() == Some(folder.uuid.as_str()) {
            child_folders += 1;
        }
    }
    let child_count = child_folders + folder.children.len();
    let folder_is_empty = child_count == 0;
    let active_metadata = active_page.clone().map(|page| PageMetadata {
        title: page.title,
        url: page.url,
        icon: page.icon,
        bg_color: page.bg_color,
    });

    rsx! {
        div {
            "data-bookmark-drop": "{uuid}",
            class: "flex flex-col",
            onpointerenter: move |_| set_bookmark_drop_target(drag_state, drop_target.clone()),
            onpointerleave: move |_| clear_bookmark_drop_target(drag_state, &leave_target),
            if editing() {
                div { class: "flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                    Icon {
                        class: if collapsed {
                            "h-4 w-4 shrink-0 rotate-0 text-muted-foreground transition-transform duration-200 ease-out"
                        } else {
                            "h-4 w-4 shrink-0 rotate-90 text-muted-foreground transition-transform duration-200 ease-out"
                        },
                        path { d: "m9 18 6-6-6-6" }
                    }
                    InlineEdit {
                        draft,
                        class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none".to_string(),
                        placeholder: translate("layout-folder-name"),
                        aria_label: translate("layout-folder-name"),
                        on_active_change: set_bookmark_text_input_active,
                        on_commit: {
                            let id = uuid.clone();
                            move |name| {
                                editing.set(false);
                                commit_folder_rename(id.clone(), name);
                            }
                        },
                        on_cancel: move |_| editing.set(false),
                    }
                }
            } else {
                BookmarkContextMenu {
                    command: "menu_folder".to_string(),
                    uuid: Some(folder.uuid.clone()),
                    metadata: active_metadata,
                    trigger: rsx! {
                        div {
                            "data-bookmark-drag-source": "true",
                            class: if folder_targeted { "rounded-md ring-2 ring-ring" } else { "rounded-md" },
                            onpointerdown: {
                                let item = drag_item.clone();
                                move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                            },
                            SidebarTreeRowGroup {
                                SidebarTreeRow {
                                    path: uuid.clone(),
                                    label: folder.name.clone(),
                                    is_dir: true,
                                    expanded: !collapsed,
                                    emphasis: true,
                                    title: folder.name.clone(),
                                    on_activate: {
                                        let id = uuid.clone();
                                        move |()| {
                                            if bookmark_drag_blocks_click(drag_state) {
                                                return;
                                            }
                                            bookmark_cmd("toggle_folder", Some(id.clone()));
                                        }
                                    },
                                    trailing: rsx! {
                                        if child_count > 0 {
                                            span { class: "shrink-0 text-[10px] tabular-nums text-muted-foreground/70",
                                                "{child_count}"
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    },
                    menu: rsx! {
                        SideSheetContextMenuContent {
                        ContextMenuItem {
                            index: 0usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let id = uuid.clone(); move |_: String| bookmark_cmd("toggle_folder", Some(id.clone())) },
                            attributes: vec![],
                            {if collapsed { translate("common-expand") } else { translate("common-collapse") }}
                        }
                        ContextMenuItem {
                            index: 1usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            disabled: active_page.is_none(),
                            on_select: {
                                let id = uuid.clone();
                                let page = active_page.clone();
                                move |_: String| {
                                    if let Some(page) = page.clone() {
                                        add_to_bookmarks(
                                            "add",
                                            PageMetadata {
                                                title: page.title,
                                                url: page.url,
                                                icon: page.icon,
                                                bg_color: page.bg_color,
                                            },
                                            Some(id.clone()),
                                        );
                                    }
                                }
                            },
                            attributes: vec![],
                            {translate("layout-bookmark-current-page")}
                        }
                        ContextMenuItem {
                            index: 2usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: move |_: String| {
                                if collapsed {
                                    bookmark_cmd("toggle_folder", Some(new_folder_uuid.clone()));
                                }
                                begin_new_folder(creating_child, child_draft);
                            },
                            attributes: vec![],
                            {translate("layout-new-folder")}
                        }
                        ContextMenuItem {
                            index: 3usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: {
                                let name = folder.name.clone();
                                move |_: String| begin_inline_rename(editing, draft, name.clone())
                            },
                            attributes: vec![],
                            {translate("layout-rename-folder")}
                        }
                        for (index, (target_folder, label)) in move_targets.iter().enumerate() {
                            ContextMenuItem {
                                key: "{index}",
                                index: 4usize + index,
                                value: Into::<ReadSignal<String>>::into(menu_val),
                                on_select: {
                                    let id = uuid.clone();
                                    let folder = target_folder.clone();
                                    move |_: String| move_bookmark_folder(id.clone(), folder.clone())
                                },
                                attributes: vec![],
                                "{label}"
                            }
                        }
                        ContextMenuItem {
                            index: remove_index,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let id = uuid.clone(); move |_: String| bookmark_cmd("remove_folder", Some(id.clone())) },
                            attributes: vec![],
                            {translate("layout-remove-folder")}
                        }
                        }
                    },
                }
            }
            if creating_child() {
                div { class: "ml-3 flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                    Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                        path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                    }
                    InlineEdit {
                        draft: child_draft,
                        class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none".to_string(),
                        placeholder: translate("layout-folder-name"),
                        aria_label: translate("layout-folder-name"),
                        on_active_change: set_bookmark_text_input_active,
                        on_commit: {
                            let parent = uuid.clone();
                            move |name| {
                                creating_child.set(false);
                                create_bookmark_folder(name, Some(parent.clone()));
                            }
                        },
                        on_cancel: move |_| creating_child.set(false),
                    }
                }
            }
            SidebarTreeChildren { expanded: !collapsed,
                div { class: "ml-3 flex flex-col gap-1",
                    for child_folder in folder_rows
                        .iter()
                        .filter(|child| child.parent.as_deref() == Some(folder.uuid.as_str()))
                    {
                        BookmarkFolder {
                            key: "{child_folder.uuid}",
                            folder: child_folder.clone(),
                            parent_uuid: Some(folder.uuid.clone()),
                            folders: folders.clone(),
                            folder_rows: folder_rows.clone(),
                            active_page: active_page.clone(),
                        }
                    }
                    for bookmark in folder.children.iter() {
                        BookmarkEntry {
                            key: "{bookmark.uuid}",
                            row: bookmark.clone(),
                            folder_uuid: Some(folder.uuid.clone()),
                            folders: folders.clone(),
                        }
                    }
                    if folder_is_empty {
                        div { class: "px-2 py-1.5 text-ui-xs text-muted-foreground", {translate("layout-empty-folder")} }
                    }
                }
            }
        }
    }
}

fn begin_new_folder(mut creating: Signal<bool>, mut draft: Signal<String>) {
    draft.set(translate("layout-new-folder"));
    creating.set(true);
}

#[component]
fn BookmarkEntry(
    row: BookmarkRow,
    folder_uuid: Option<String>,
    folders: Vec<BookmarkFolderChoice>,
) -> Element {
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let url_open = row.metadata.url.clone();
    let uuid_pin = row.uuid.clone();
    let uuid_remove = row.uuid.clone();
    let uuid_rename = row.uuid.clone();
    let menu_val = use_signal(|| row.uuid.clone());
    let title = if row.metadata.title.is_empty() {
        row.metadata.url.clone()
    } else {
        row.metadata.title.clone()
    };
    let mut editing = use_signal(|| false);
    let draft = use_signal(|| title.clone());
    let bookmark_menu_action: Signal<BookmarkMenuActionEvent> = use_context();
    let initial_menu_action = bookmark_menu_action.peek().sequence;
    let mut handled_menu_action = use_signal(|| initial_menu_action);
    let menu_action_uuid = row.uuid.clone();
    let menu_action_name = title.clone();
    use_effect(move || {
        let action = bookmark_menu_action();
        if action.sequence == handled_menu_action() {
            return;
        }
        handled_menu_action.set(action.sequence);
        if action.action == "rename" && action.uuid.as_deref() == Some(menu_action_uuid.as_str()) {
            begin_inline_rename(editing, draft, menu_action_name.clone());
        }
    });
    let mut move_targets: Vec<(Option<String>, String)> = Vec::new();
    if folder_uuid.is_some() {
        move_targets.push((None, translate("layout-move-to-bookmarks")));
    }
    move_targets.extend(
        folders
            .iter()
            .filter(|folder| Some(folder.uuid.as_str()) != folder_uuid.as_deref())
            .map(|folder| {
                (
                    Some(folder.uuid.clone()),
                    translate_with(
                        "layout-move-to",
                        &[("folder", TranslationValue::String(&folder.label))],
                    ),
                )
            }),
    );
    let remove_index = 3 + move_targets.len();
    let drag_item = BookmarkDragItem::Bookmark {
        uuid: row.uuid.clone(),
    };
    rsx! {
        if editing() {
            div { class: "flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                PageIconView {
                    icon: row.metadata.icon.clone(),
                    url: row.metadata.url.clone(),
                    img_class: "h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
                    icon_class: "h-4 w-4 shrink-0 text-muted-foreground".to_string(),
                }
                InlineEdit {
                    draft,
                    class: "min-w-0 flex-1 bg-transparent text-ui text-foreground outline-none".to_string(),
                    placeholder: String::new(),
                    aria_label: translate("common-rename"),
                    on_active_change: set_bookmark_text_input_active,
                    on_commit: {
                        let id = uuid_rename.clone();
                        move |name| {
                            editing.set(false);
                            commit_bookmark_rename(id.clone(), name);
                        }
                    },
                    on_cancel: move |_| editing.set(false),
                }
            }
        } else {
            BookmarkContextMenu {
                command: "menu_bookmark".to_string(),
                uuid: Some(row.uuid.clone()),
                metadata: None,
                trigger: rsx! {
                    div {
                        "data-bookmark-drag-source": "true",
                        onpointerdown: {
                            let item = drag_item.clone();
                            move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                        },
                        SidebarTreeRowGroup {
                            SidebarTreeRow {
                                path: row.metadata.url.clone(),
                                label: title.clone(),
                                is_dir: false,
                                title: title.clone(),
                                leading: rsx! {
                                    PageIconView {
                                        icon: row.metadata.icon.clone(),
                                        url: row.metadata.url.clone(),
                                        img_class: "h-3.5 w-3.5 shrink-0 rounded-sm object-contain".to_string(),
                                        icon_class: "h-3.5 w-3.5 shrink-0 text-muted-foreground".to_string(),
                                    }
                                },
                                on_activate: {
                                    let u = url_open.clone();
                                    move |()| {
                                        if bookmark_drag_blocks_click(drag_state) {
                                            return;
                                        }
                                        open_bookmark(u.clone());
                                    }
                                },
                            }
                        }
                    }
                },
                menu: rsx! {
                    SideSheetContextMenuContent {
                    ContextMenuItem {
                        index: 0usize,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: { let u = url_open.clone(); move |_: String| open_bookmark(u.clone()) },
                        attributes: vec![],
                        {translate("common-open")}
                    }
                    ContextMenuItem {
                        index: 1usize,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: {
                            let name = title.clone();
                            move |_: String| begin_inline_rename(editing, draft, name.clone())
                        },
                        attributes: vec![],
                        {translate("common-rename")}
                    }
                    ContextMenuItem {
                        index: 2usize,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: {
                            let id = uuid_pin.clone();
                            let command = if row.pinned { "unpin" } else { "pin" };
                            move |_: String| bookmark_cmd(command, Some(id.clone()))
                        },
                        attributes: vec![],
                        {if row.pinned { translate("layout-unpin-page") } else { translate("layout-pin") }}
                    }
                    for (index, (target_folder, label)) in move_targets.iter().enumerate() {
                        ContextMenuItem {
                            key: "{index}",
                            index: 3usize + index,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: {
                                let id = row.uuid.clone();
                                let folder = target_folder.clone();
                                move |_: String| move_bookmark(id.clone(), folder.clone())
                            },
                            attributes: vec![],
                            "{label}"
                        }
                    }
                    ContextMenuItem {
                        index: remove_index,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: { let id = uuid_remove.clone(); move |_: String| bookmark_cmd("remove", Some(id.clone())) },
                        attributes: vec![],
                        {translate("common-remove")}
                    }
                    }
                },
            }
        }
    }
}

fn request_bookmark_menu(command: &str, uuid: Option<String>, metadata: Option<PageMetadata>) {
    let _ = send(&BookmarksCommandEvent {
        command: command.to_string(),
        uuid,
        name: None,
        url: None,
        metadata,
        folder: None,
        target_uuid: None,
    });
}

fn commit_folder_rename(uuid: String, name: String) {
    let name = name.trim().to_string();
    let command = if name.is_empty() {
        "remove_folder"
    } else {
        "rename_folder"
    };
    let _ = send(&BookmarksCommandEvent {
        command: command.into(),
        uuid: Some(uuid),
        name: if name.is_empty() { None } else { Some(name) },
        url: None,
        metadata: None,
        folder: None,
        target_uuid: None,
    });
}

#[component]
fn LayoutContextMenu(children: Element) -> Element {
    rsx! {
        ContextMenu {
            attributes: vec![
                dioxus_elements::events::oncontextmenu(move |event: Event<MouseData>| {
                    event.stop_propagation();
                }),
            ],
            on_open_change: set_bookmark_context_menu_active,
            {children}
        }
    }
}

#[component]
fn BookmarkContextMenu(
    command: String,
    uuid: Option<String>,
    metadata: Option<PageMetadata>,
    trigger: Element,
    menu: Element,
) -> Element {
    #[cfg(target_os = "macos")]
    {
        let _ = menu;
        return rsx! {
            div {
                role: "button",
                aria_haspopup: "menu",
                user_select: "none",
                oncontextmenu: move |event: Event<MouseData>| {
                    event.prevent_default();
                    event.stop_propagation();
                    request_bookmark_menu(&command, uuid.clone(), metadata.clone());
                },
                {trigger}
            }
        };
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = command;
        let _ = uuid;
        let _ = metadata;
        rsx! {
            LayoutContextMenu {
                ContextMenuTrigger { attributes: vec![], {trigger} }
                {menu}
            }
        }
    }
}

#[component]
fn SideSheetStackRow(stack: StackNode, pane_id: u64) -> Element {
    let folder_context: Signal<Vec<BookmarkFolderChoice>> = use_context();
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let folders = folder_context();
    let is_active = stack.is_active;
    let stack_id = stack.id;
    let command = StackCommand::new(pane_id, stack_id);
    let mut hovered = use_signal(|| false);
    let menu_val = use_signal(|| stack.url.clone());
    let metadata = PageMetadata {
        title: stack.title.clone(),
        url: stack.url.clone(),
        icon: stack.icon.clone(),
        bg_color: stack.bg_color.clone(),
    };
    let drag_item = BookmarkDragItem::Page {
        metadata: metadata.clone(),
    };
    let bookmark_metadata = metadata.clone();
    let pin_metadata = metadata;
    let pin_index = 1 + folders.len();
    let display_title = StackTitle::of(&stack);
    let close_title = translate("layout-close-stack");

    let title_class = if is_active {
        format!(
            "min-w-0 flex-1 {} text-ui font-medium text-foreground",
            dir_truncate_class(&display_title)
        )
    } else {
        format!(
            "min-w-0 flex-1 {} text-ui",
            dir_truncate_class(&display_title)
        )
    };

    rsx! {
        LayoutContextMenu {
            ContextMenuTrigger {
                attributes: vec![],
                div {
                    "data-bookmark-drag-source": "true",
                    onpointerdown: {
                        let item = drag_item.clone();
                        move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                    },
                    id: "sidesheet-stack-{pane_id}-{stack_id}",
                    class: if is_active {
                        "flex h-9 cursor-default items-center gap-2 rounded-md bg-primary/[0.07] px-2 shadow-[inset_2px_0_0_var(--primary)] ring-1 ring-inset ring-primary/15"
                    } else {
                        "flex h-9 cursor-pointer items-center gap-2 rounded-md px-2 border border-transparent text-muted-foreground hover:bg-glass-hover hover:text-foreground"
                    },
                    onmouseenter: move |_| hovered.set(true),
                    onmouseleave: move |_| hovered.set(false),
                    onclick: move |event| {
                        if bookmark_drag_blocks_click(drag_state) {
                            event.prevent_default();
                            event.stop_propagation();
                            return;
                        }
                        command.activate();
                    },
                    StackIcon { icon: stack.icon.clone(), url: stack.url.clone(), title: stack.title.clone() }
                    span { class: "{title_class}", "{display_title}" }
                    button {
                        r#type: "button",
                        aria_label: "{close_title}",
                        title: "{close_title}",
                        class: if hovered() {
                            "ml-auto flex h-6 w-6 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-100 transition-opacity focus-visible:opacity-100 hover:bg-foreground/10"
                        } else {
                            "ml-auto flex h-6 w-6 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity focus-visible:opacity-100 hover:bg-foreground/10"
                        },
                        onmousedown: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                        },
                        onpointerdown: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                        },
                        onclick: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                            command.close();
                        },
                        Icon { class: "h-3 w-3 pointer-events-none",
                            path { d: "M18 6 6 18" }
                            path { d: "m6 6 12 12" }
                        }
                    }
                }
            }
            SideSheetContextMenuContent {
                ContextMenuItem {
                    index: 0usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| add_to_bookmarks(
                        "add",
                        bookmark_metadata.clone(),
                        None,
                    ),
                    attributes: vec![],
                    {translate("layout-bookmark")}
                }
                for (index, folder) in folders.iter().enumerate() {
                    ContextMenuItem {
                        key: "{folder.uuid}",
                        index: 1usize + index,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: {
                            let metadata = PageMetadata {
                                title: stack.title.clone(),
                                url: stack.url.clone(),
                                icon: stack.icon.clone(),
                                bg_color: stack.bg_color.clone(),
                            };
                            let folder_uuid = folder.uuid.clone();
                            move |_: String| add_to_bookmarks(
                                "add",
                                metadata.clone(),
                                Some(folder_uuid.clone()),
                            )
                        },
                        attributes: vec![],
                    {translate_with(
                        "layout-bookmark-in",
                        &[("folder", TranslationValue::String(&folder.label))],
                    )}
                    }
                }
                ContextMenuItem {
                    index: pin_index,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| add_to_bookmarks(
                        "pin_url",
                        pin_metadata.clone(),
                        None,
                    ),
                    attributes: vec![],
                    {translate("layout-pin")}
                }
            }
        }
    }
}

#[component]
fn NewStackRow(pane_id: u64) -> Element {
    rsx! {
        SheetNewButton {
            label: translate("layout-new-stack"),
            icon: rsx! {
                Icon { class: "h-4 w-4 shrink-0",
                    path { d: "M12 5v14" }
                    path { d: "M5 12h14" }
                }
            },
            onclick: move |_| {
                let _ = send(&crate::event::SideSheetCommandEvent {
                    command: "new_stack".to_string(),
                    pane_id: pane_id.to_string(),
                    stack_id: 0,
                    line: 0,
                    path: String::new(),
                });
            },
        }
    }
}

#[component]
fn StackIcon(icon: PageIcon, url: String, title: String) -> Element {
    if title == "New Stack" && url.is_empty() {
        return rsx! {
            Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                path { d: "M5 12h14" }
                path { d: "M12 5v14" }
            }
        };
    }
    rsx! {
        PageIconView {
            icon,
            url,
            img_class: "h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
            icon_class: "h-4 w-4 shrink-0 text-muted-foreground".to_string(),
        }
    }
}

#[component]
fn SideSheetContextMenuContent(children: Element) -> Element {
    rsx! {
        ContextMenuContent { attributes: vec![], {children} }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct StackCommand {
    pane_id: u64,
    stack_id: u64,
}

impl StackCommand {
    fn new(pane_id: u64, stack_id: u64) -> Self {
        Self { pane_id, stack_id }
    }

    fn activate(self) {
        self.dispatch("activate_stack");
    }

    fn close(self) {
        self.dispatch("close_stack");
    }

    fn dispatch(self, command: &str) {
        let _ = send(&crate::event::SideSheetCommandEvent {
            command: command.to_string(),
            pane_id: self.pane_id.to_string(),
            stack_id: self.stack_id,
            line: 0,
            path: String::new(),
        });
    }
}

struct StackTitle;

impl StackTitle {
    fn of(stack: &StackNode) -> String {
        let title = localized_stack_title(stack);
        if !title.trim().is_empty() {
            return title;
        }
        if let Some(host) = vmux_ui::favicon::host_for_favicon_fallback(&stack.url) {
            return host.to_string();
        }
        if stack.is_loading {
            return translate("layout-loading");
        }
        if stack.url.is_empty() {
            return translate("layout-new-stack");
        }
        stack.url.clone()
    }
}

fn localized_stack_title(stack: &StackNode) -> String {
    match stack.url.trim_end_matches('/') {
        "vmux://start" => translate("start-title"),
        "vmux://settings" => translate("settings-title"),
        _ if stack.url.is_empty() && stack.title == "New Stack" => translate("layout-new-stack"),
        _ => stack.title.clone(),
    }
}

fn download_pct(downloaded: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    (downloaded.saturating_mul(100) / total).min(100)
}

#[derive(Clone, PartialEq)]
enum UpdatePhase {
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
    },
    Installing {
        version: String,
    },
    Ready {
        version: String,
    },
}

#[component]
fn SheetNewButton(label: String, icon: Element, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "group flex h-9 cursor-pointer items-center gap-2 rounded-md px-2 border border-transparent text-left text-muted-foreground hover:bg-glass-hover hover:text-foreground",
            onclick: move |e| onclick.call(e),
            {icon}
            span { class: "min-w-0 flex-1 truncate text-ui font-medium", "{label}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_url_matches_with_or_without_a_trailing_slash() {
        assert!(PageUrl::matches("vmux://start", "vmux://start/"));
        assert!(PageUrl::matches("vmux://start/", "vmux://start"));
        assert!(!PageUrl::matches("vmux://start", "vmux://projects"));
    }

    #[test]
    fn active_session_matches_the_agent_by_session_id() {
        let team = vec![
            TeamMemberRow {
                id: "first".into(),
                name: "Codex one".into(),
                url: "vmux://sessions/codex/".into(),
                sid: "session-one".into(),
                ..Default::default()
            },
            TeamMemberRow {
                id: "second".into(),
                name: "Codex two".into(),
                url: "vmux://sessions/codex/".into(),
                sid: "session-two".into(),
                ..Default::default()
            },
        ];

        let agent =
            ActiveSessionInfo::agent_for("vmux://sessions/codex/cli/session-two", &team).unwrap();

        assert_eq!(agent.id, "second");
    }

    #[test]
    fn download_pct_clamps_and_handles_zero_total() {
        assert_eq!(download_pct(0, 0), 0);
        assert_eq!(download_pct(50, 100), 50);
        assert_eq!(download_pct(250, 100), 100);
    }

    #[test]
    fn tab_drag_shifts_the_tabs_between_source_and_target() {
        let mut state = TabDragState {
            source_id: "a".into(),
            source_index: 0,
            target_index: 0,
            order: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            start_x: 100.0,
            start_y: 0.0,
            current_x: 100.0 + TAB_STEP_PX * 2.1,
            active: true,
        };

        state.update_target();

        assert_eq!(state.target_index, 2);
        assert_eq!(state.offset_for(1), -TAB_STEP_PX);
        assert_eq!(state.offset_for(2), -TAB_STEP_PX);
        assert_eq!(state.offset_for(3), 0.0);
    }

    #[test]
    fn tab_drag_clamps_to_the_available_slots() {
        let mut state = TabDragState {
            source_id: "c".into(),
            source_index: 2,
            target_index: 2,
            order: vec!["a".into(), "b".into(), "c".into()],
            start_x: 100.0,
            start_y: 0.0,
            current_x: -1000.0,
            active: true,
        };

        state.update_target();

        assert_eq!(state.target_index, 0);
        assert_eq!(state.offset_for(0), TAB_STEP_PX);
        assert_eq!(state.offset_for(1), TAB_STEP_PX);
        assert_eq!(state.source_offset(), -TAB_STEP_PX * 2.0);
    }

    #[test]
    fn optimistic_pin_order_moves_the_source_to_the_target_slot() {
        let order = vec!["a".into(), "b".into(), "c".into(), "d".into()];

        let optimistic = OptimisticPinOrder::after_drop(&order, 0, 2).unwrap();

        assert_eq!(optimistic.baseline, order);
        assert_eq!(optimistic.expected, ["b", "c", "a", "d"]);
    }

    #[test]
    fn optimistic_pin_order_ignores_a_drop_on_the_same_slot() {
        let order = vec!["a".into(), "b".into()];

        assert!(OptimisticPinOrder::after_drop(&order, 1, 1).is_none());
    }

    fn state(header_open: bool, side_sheet_open: bool) -> LayoutStateEvent {
        LayoutStateEvent {
            header_open,
            side_sheet_open,
            ..Default::default()
        }
    }

    #[test]
    fn overlay_waits_for_layout_state() {
        assert!(!layout_overlay_ready(
            &state(false, false),
            false,
            true,
            true,
            true,
            true
        ));
    }

    #[test]
    fn overlay_waits_for_header_state_when_header_visible() {
        let visible = state(true, false);

        assert!(!layout_overlay_ready(
            &visible, true, false, true, true, true
        ));
        assert!(!layout_overlay_ready(
            &visible, true, true, false, true, true
        ));
        assert!(layout_overlay_ready(&visible, true, true, true, true, true));
    }

    #[test]
    fn overlay_waits_for_side_sheet_state_when_side_sheet_visible() {
        let visible = state(false, true);

        assert!(!layout_overlay_ready(
            &visible, true, true, true, false, true
        ));
        assert!(!layout_overlay_ready(
            &visible, true, true, true, true, false
        ));
        assert!(layout_overlay_ready(&visible, true, true, true, true, true));
    }

    #[test]
    fn overlay_can_be_ready_when_overlay_is_closed() {
        assert!(layout_overlay_ready(
            &state(false, false),
            true,
            false,
            false,
            false,
            false
        ));
    }
}
