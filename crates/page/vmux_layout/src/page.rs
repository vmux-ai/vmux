#![allow(non_snake_case)]

mod state;
mod tab_drag;
mod update;
mod window_drag;

use std::rc::Rc;

use self::state::LayoutPageState;
use self::tab_drag::TabDrag;
use self::update::UpdateNoticeFooter;
use self::window_drag::WindowDragRegion;
use crate::active_session::ActiveSessionPanel;
use crate::event::{
    HeaderRequest, LayoutStateEvent, PaneNode, PaneTreeEvent, ReloadEvent, RemoteStateEvent,
    StackNode, StackRow, StacksHostEvent, TabRow, TabsHostEvent, TabsRequest,
};
use crate::extension::{ExtensionBar, ExtensionPopupModal};
use crate::remote::RemoteControl;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_api::bookmark::{
    BookmarkAddRequest, BookmarkContextMenuRequest, BookmarkFolderCreateRequest,
    BookmarkFolderMoveRequest, BookmarkFolderRemoveRequest, BookmarkFolderRenameRequest,
    BookmarkFolderRow, BookmarkFolderToggleRequest, BookmarkMenuActionEvent,
    BookmarkMenuEntryRequest, BookmarkMenuFolderRequest, BookmarkMenuPinRequest,
    BookmarkMenuRootRequest, BookmarkMovePinRequest, BookmarkMoveRequest, BookmarkNode,
    BookmarkOpenRequest, BookmarkPinRequest, BookmarkPinUrlRequest, BookmarkRemoveRequest,
    BookmarkRenameRequest, BookmarkReorderPinRequest, BookmarkRow, BookmarkStateEvent,
    BookmarkTextInputRequest, BookmarkToggleRequest, BookmarkUnpinRequest,
};
use vmux_command::panel::CommandBarPanel;
use vmux_core::event::ExtRow;
use vmux_core::event::team::{TeamMemberRow, TeamRequest};
use vmux_core::{PageIcon, PageMetadata};
use vmux_ui::components::avatar::Avatar;
use vmux_ui::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use vmux_ui::components::icon::Icon;
use vmux_ui::components::inline_edit::{EditableText, InlineEdit};
use vmux_ui::components::tree_row::{
    SIDEBAR_CARD_CHEVRON_CLOSED, SIDEBAR_CARD_CHEVRON_OPEN, SIDEBAR_TREE_COLUMN,
    SIDEBAR_TREE_SCROLLER, SidebarTreeChildren, SidebarTreeRow, SidebarTreeRowGroup,
};
use vmux_ui::favicon::{Favicon, favicon_src_for_url};
use vmux_ui::hooks::{send, use_listener, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::PageIconView;
use vmux_ui::platform::sleep_ms;
use vmux_ui::scroll::ScrollIntoView;
use vmux_ui::util::cn;

#[component]
pub fn Page() -> Element {
    use_theme();
    let layout_ui = LayoutPageState::use_state();
    let bookmark_menu_action = use_ui_state::<BookmarkMenuActionEvent>();
    use_context_provider(|| bookmark_menu_action);

    let mut reload_key = use_signal(|| 0u32);
    let _reload_listener = use_listener::<ReloadEvent, _>(move |_| {
        reload_key.set(reload_key() + 1);
    });

    let ui = layout_ui.value();
    let layout_ready = ui.layout.is_some();
    let stacks_ready = ui.stacks.is_some();
    let tabs_ready = ui.tabs.is_some();
    let pane_tree_ready = ui.pane_tree.is_some();
    let spaces_ready = ui.spaces.is_some();
    let state = ui.layout.unwrap_or_default();
    let stacks = ui.stacks.unwrap_or_default();
    let tabs = ui.tabs.unwrap_or_default();
    let bookmarks = ui.bookmarks;
    let PaneTreeEvent { panes } = ui.pane_tree.unwrap_or_default();
    let active_space = ui
        .spaces
        .unwrap_or_default()
        .spaces
        .into_iter()
        .find(|space| space.is_active);
    let projects = ui.projects;
    let team = ui.team;
    let remote = ui.remote;
    let extensions = ui.extensions;
    let extension_popup = ui.extension_popup;
    let extension_popup_size = ui.extension_popup_size;
    let update_phase = ui.update;
    let ui_error = layout_ui.error();
    let overlay_ready = layout_overlay_ready(
        &state,
        listener_ready(layout_ready, &ui_error),
        listener_ready(stacks_ready, &ui_error),
        listener_ready(tabs_ready, &ui_error),
        listener_ready(pane_tree_ready, &ui_error),
        listener_ready(spaces_ready, &ui_error),
    );
    let radius_px = state.radius;
    let reveal = StackReveal::side_sheet(use_signal(|| None::<(u64, u64)>));
    use_effect(move || set_root_radius_px(radius_px));
    use_effect(move || {
        let state = layout_ui.value();
        if !state.layout.unwrap_or_default().side_sheet_open {
            reveal.forget();
            return;
        }
        let PaneTreeEvent { panes } = state.pane_tree.unwrap_or_default();
        let Some(target) = ActiveStack::find(&panes) else {
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
                    WindowDragRegion {
                        id: "side-sheet-titlebar",
                        revision: sheet_width().to_string(),
                        class: "pointer-events-none absolute top-0 h-7",
                        style: "left:80px;right:4px;",
                    }
                    SideSheetGrab { resizing: sheet_resizing }
                    div { class: "flex h-full min-h-0 flex-col",
                        SideSheetView {
                            panes,
                            active_space,
                            bookmarks: bookmarks.clone(),
                            projects: projects.projects.clone(),
                            boundary: projects.boundary,
                            team: team.members.clone(),
                            pane_tree_error: ui_error.clone(),
                        }
                        if let Some(phase) = update_phase.clone() {
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
                        bookmarks,
                        team: team.members,
                        extensions: extensions.extensions,
                        remote,
                        reload_key: reload_key(),
                        stacks_error: ui_error.clone(),
                        tabs_error: ui_error.clone(),
                    }
                }
            }
            CommandBarPanel {}
            if !extension_popup.id.is_empty() {
                ExtensionPopupModal {
                    popup: extension_popup,
                    preferred_size: extension_popup_size,
                }
            }
            if sheet_resizing() {
                div {
                    class: "pointer-events-auto fixed inset-0 z-[900] cursor-col-resize",
                    onmousemove: move |event: Event<MouseData>| {
                        let x = event.client_coordinates().x as f32 - sheet_left;
                        let width = crate::event::SideSheetResizeEvent::live(x).clamped();
                        sheet_width.set(width);
                        let _ = send(&crate::event::SideSheetResizeEvent::live(width));
                    },
                    onmouseup: move |_| {
                        sheet_resizing.set(false);
                        let _ = send(&crate::event::SideSheetResizeEvent::settled(sheet_width()));
                    },
                }
            }
        }
    }
}

#[component]
fn SideSheetGrab(mut resizing: Signal<bool>) -> Element {
    let handle_class = if resizing() {
        "relative flex h-10 w-2 items-center justify-center rounded-full bg-primary/20 shadow-sm ring-1 ring-primary/50"
    } else {
        "relative flex h-8 w-1.5 items-center justify-center rounded-full bg-background/80 opacity-0 shadow-sm ring-1 ring-foreground/15 transition-all duration-150 group-hover:h-10 group-hover:w-2 group-hover:bg-primary/15 group-hover:opacity-100 group-hover:ring-primary/45"
    };
    let line_class = if resizing() {
        "absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-primary/45"
    } else {
        "absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-primary/45 opacity-0 transition-opacity duration-150 group-hover:opacity-100"
    };
    rsx! {
        div {
            class: "group absolute inset-y-0 z-20 flex w-6 cursor-col-resize items-center justify-center",
            style: "right:-10px;",
            onmousedown: move |event: Event<MouseData>| {
                event.prevent_default();
                resizing.set(true);
            },
            div { class: "{line_class}" }
            div { class: "{handle_class}",
                div { class: "h-5 w-px rounded-full bg-foreground/40 transition-colors duration-150 group-hover:bg-primary/90" }
            }
        }
    }
}

struct ActiveStack;

impl ActiveStack {
    fn find(panes: &[PaneNode]) -> Option<(u64, u64)> {
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
    bookmarks: BookmarkStateEvent,
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
    let folders = BookmarkFolderChoice::all(&bookmarks.roots);
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
fn HeaderView(
    stacks_state: StacksHostEvent,
    tabs_state: TabsHostEvent,
    bookmarks: BookmarkStateEvent,
    team: Vec<TeamMemberRow>,
    extensions: Vec<ExtRow>,
    remote: RemoteStateEvent,
    reload_key: u32,
    stacks_error: Option<String>,
    tabs_error: Option<String>,
) -> Element {
    let tab_drag = TabDrag::use_state();
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
    let tab_metrics_style = TabDrag::metrics_style();
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
                    NavButton { label: translate("layout-back"), command: HeaderRequest::PreviousPage, disabled: !can_go_back,
                        Icon { class: "h-4 w-4",
                            path { d: "M19 12H5" }
                            path { d: "M12 19l-7-7 7-7" }
                        }
                    }
                    NavButton { label: translate("layout-forward"), command: HeaderRequest::NextPage, disabled: !can_go_forward,
                        Icon { class: "h-4 w-4",
                            path { d: "M5 12h14" }
                            path { d: "M12 5l7 7-7 7" }
                        }
                    }
                    NavButton { label: translate("layout-reload"), command: HeaderRequest::Reload, disabled: active_row.as_ref().is_none_or(|t| t.url.is_empty()),
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
                                let _ = send(&BookmarkToggleRequest);
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
                                    bookmark_cmd(BookmarkIdCommand::Unpin, uuid);
                                } else if let Some(metadata) = active_metadata.clone() {
                                    add_to_bookmarks(BookmarkPageCommand::Pin, metadata, None);
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
                            let _ = send(&vmux_core::event::space::SpaceRequest::OpenPage);
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
                                let _ = send(&vmux_core::event::space::SpaceRequest::Rename {
                                    space_id: rename_id.clone(),
                                    name,
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

#[component]
fn BookmarksSection(
    bookmarks: BookmarkStateEvent,
    active_page: Option<StackNode>,
    pane_id: u64,
    expanded: bool,
) -> Element {
    let BookmarkStateEvent { pins, roots } = bookmarks;
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
    let folders = BookmarkFolderChoice::all(&roots);
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
                BookmarkContextTarget::Root.request();
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
                                        active: active_url.as_ref().is_some_and(|active_url| {
                                            let Some(active) = vmux_api::VmuxRoute::parse(active_url) else {
                                                return false;
                                            };
                                            vmux_api::VmuxRoute::parse(&pin.metadata.url)
                                                .is_some_and(|pin| active.same_page(&pin))
                                        }),
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
    let request = if expanded {
        crate::event::SideSheetRequest::ExpandSection {
            pane_id,
            path: section.to_string(),
        }
    } else {
        crate::event::SideSheetRequest::CollapseSection {
            pane_id,
            path: section.to_string(),
        }
    };
    let _ = send(&request);
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
    fn resolve(state: Option<&BookmarkDragState>, uuid: &str, index: usize) -> Self {
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

fn bookmark_folder_rows(nodes: &[BookmarkNode]) -> Vec<BookmarkFolderRow> {
    nodes
        .iter()
        .filter_map(|node| match node {
            BookmarkNode::Folder(folder) => Some(folder.clone()),
            BookmarkNode::Entry(_) => None,
        })
        .collect()
}

impl BookmarkFolderChoice {
    fn all(nodes: &[BookmarkNode]) -> Vec<Self> {
        let folders = bookmark_folder_rows(nodes);
        let mut output = Vec::new();
        Self::collect(
            &folders,
            None,
            "",
            &[],
            &mut std::collections::HashSet::new(),
            &mut output,
        );
        output
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
    let inactive_hover_classes = if visual.active() {
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
                        let _ = send(&TabsRequest::Close {
                            tab_id: Some(id_close.clone()),
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
                    on_select: move |_: String| add_to_bookmarks(BookmarkPageCommand::Add, bookmark_metadata.clone(), None),
                    attributes: vec![],
                    {translate("layout-bookmark")}
                }
                ContextMenuItem {
                    index: 1usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| add_to_bookmarks(BookmarkPageCommand::Pin, pin_metadata.clone(), None),
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
                    let _ = send(&TabsRequest::New);
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
fn NavButton(
    label: String,
    command: HeaderRequest,
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
                    let _ = send(&command);
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
                let _ = send(&HeaderRequest::FocusAddressBar);
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
                        let _ = send(&TeamRequest {
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
                                        let _ = send(&TeamRequest {
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
                                let _ = send(&TeamRequest {
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
    let visual = PinDragVisual::resolve(drag_state().as_ref(), &row.uuid, index);
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
                target: BookmarkContextTarget::Pin { uuid: row.uuid.clone() },
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
                        if vmux_api::VmuxRoute::parse(&row.metadata.url).is_some() {
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
                            on_select: { let id = uuid_unpin.clone(); move |_: String| bookmark_cmd(BookmarkIdCommand::Unpin, id.clone()) },
                            attributes: vec![],
                            {translate("layout-unpin-page")}
                        }
                        if row.bookmarked {
                            ContextMenuItem {
                                index: 2usize,
                                value: Into::<ReadSignal<String>>::into(menu_val),
                                on_select: { let id = row.uuid.clone(); move |_: String| bookmark_cmd(BookmarkIdCommand::Remove, id.clone()) },
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
    let _ = send(&BookmarkOpenRequest { url });
}

#[derive(Clone, Copy)]
enum BookmarkIdCommand {
    Remove,
    Pin,
    Unpin,
    ToggleFolder,
    RemoveFolder,
}

fn bookmark_cmd(command: BookmarkIdCommand, uuid: String) {
    match command {
        BookmarkIdCommand::Remove => {
            let _ = send(&BookmarkRemoveRequest { uuid });
        }
        BookmarkIdCommand::Pin => {
            let _ = send(&BookmarkPinRequest { uuid });
        }
        BookmarkIdCommand::Unpin => {
            let _ = send(&BookmarkUnpinRequest { uuid });
        }
        BookmarkIdCommand::ToggleFolder => {
            let _ = send(&BookmarkFolderToggleRequest { uuid });
        }
        BookmarkIdCommand::RemoveFolder => {
            let _ = send(&BookmarkFolderRemoveRequest { uuid });
        }
    }
}

#[derive(Clone, Copy)]
enum BookmarkPageCommand {
    Add,
    Pin,
}

fn add_to_bookmarks(command: BookmarkPageCommand, metadata: PageMetadata, folder: Option<String>) {
    match command {
        BookmarkPageCommand::Add => {
            let _ = send(&BookmarkAddRequest { metadata, folder });
        }
        BookmarkPageCommand::Pin => {
            let _ = send(&BookmarkPinUrlRequest { metadata });
        }
    }
}

fn move_bookmark(uuid: String, folder: Option<String>) {
    let _ = send(&BookmarkMoveRequest { uuid, folder });
}

fn move_pin(uuid: String, folder: Option<String>) {
    let _ = send(&BookmarkMovePinRequest { uuid, folder });
}

fn reorder_pin(uuid: String, target_uuid: String) -> bool {
    send(&BookmarkReorderPinRequest { uuid, target_uuid }).is_ok()
}

fn move_bookmark_folder(uuid: String, folder: Option<String>) {
    let _ = send(&BookmarkFolderMoveRequest {
        uuid,
        parent: folder,
    });
}

fn commit_bookmark_rename(uuid: String, name: String) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let _ = send(&BookmarkRenameRequest { uuid, name });
}

fn create_bookmark_folder(name: String, parent: Option<String>) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let _ = send(&BookmarkFolderCreateRequest { name, parent });
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
            add_to_bookmarks(BookmarkPageCommand::Add, metadata, None)
        }
        (BookmarkDragItem::Page { metadata }, BookmarkDropTarget::Folder(folder)) => {
            add_to_bookmarks(BookmarkPageCommand::Add, metadata, Some(folder))
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
    let _ = send(&BookmarkTextInputRequest { active });
}

fn set_bookmark_context_menu_active(active: bool) {
    let _ = send(&BookmarkContextMenuRequest { active });
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
    folder: BookmarkFolderRow,
    parent_uuid: Option<String>,
    folders: Vec<BookmarkFolderChoice>,
    folder_rows: Vec<BookmarkFolderRow>,
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
                    target: BookmarkContextTarget::Folder {
                        uuid: folder.uuid.clone(),
                        active_page: active_metadata,
                    },
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
                                            bookmark_cmd(BookmarkIdCommand::ToggleFolder, id.clone());
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
                            on_select: { let id = uuid.clone(); move |_: String| bookmark_cmd(BookmarkIdCommand::ToggleFolder, id.clone()) },
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
                                            BookmarkPageCommand::Add,
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
                                    bookmark_cmd(BookmarkIdCommand::ToggleFolder, new_folder_uuid.clone());
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
                            on_select: { let id = uuid.clone(); move |_: String| bookmark_cmd(BookmarkIdCommand::RemoveFolder, id.clone()) },
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
                target: BookmarkContextTarget::Entry { uuid: row.uuid.clone() },
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
                            let command = if row.pinned {
                                BookmarkIdCommand::Unpin
                            } else {
                                BookmarkIdCommand::Pin
                            };
                            move |_: String| bookmark_cmd(command, id.clone())
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
                        on_select: { let id = uuid_remove.clone(); move |_: String| bookmark_cmd(BookmarkIdCommand::Remove, id.clone()) },
                        attributes: vec![],
                        {translate("common-remove")}
                    }
                    }
                },
            }
        }
    }
}

fn commit_folder_rename(uuid: String, name: String) {
    let name = name.trim().to_string();
    if name.is_empty() {
        let _ = send(&BookmarkFolderRemoveRequest { uuid });
    } else {
        let _ = send(&BookmarkFolderRenameRequest { uuid, name });
    }
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
fn BookmarkContextMenu(target: BookmarkContextTarget, trigger: Element, menu: Element) -> Element {
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
                    target.request();
                },
                {trigger}
            }
        };
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = target;
        rsx! {
            LayoutContextMenu {
                ContextMenuTrigger { attributes: vec![], {trigger} }
                {menu}
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum BookmarkContextTarget {
    Root,
    Pin {
        uuid: String,
    },
    Entry {
        uuid: String,
    },
    Folder {
        uuid: String,
        active_page: Option<PageMetadata>,
    },
}

impl BookmarkContextTarget {
    fn request(&self) {
        match self {
            Self::Root => {
                let _ = send(&BookmarkMenuRootRequest);
            }
            Self::Pin { uuid } => {
                let _ = send(&BookmarkMenuPinRequest { uuid: uuid.clone() });
            }
            Self::Entry { uuid } => {
                let _ = send(&BookmarkMenuEntryRequest { uuid: uuid.clone() });
            }
            Self::Folder { uuid, active_page } => {
                let _ = send(&BookmarkMenuFolderRequest {
                    uuid: uuid.clone(),
                    active_page: active_page.clone(),
                });
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
    let display_title = StackTitle::resolve(&stack);
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
                        BookmarkPageCommand::Add,
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
                                BookmarkPageCommand::Add,
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
                        BookmarkPageCommand::Pin,
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
                let _ = send(&crate::event::SideSheetRequest::NewStack { pane_id });
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
        let _ = send(&crate::event::SideSheetRequest::ActivateStack {
            pane_id: self.pane_id,
            stack_id: self.stack_id,
        });
    }

    fn close(self) {
        let _ = send(&crate::event::SideSheetRequest::CloseStack {
            pane_id: self.pane_id,
            stack_id: self.stack_id,
        });
    }
}

struct StackTitle;

impl StackTitle {
    fn resolve(stack: &StackNode) -> String {
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
    if let Some(route) = vmux_api::VmuxRoute::parse(&stack.url) {
        if route.is_host("start") && route.is_root() {
            return translate("start-title");
        }
        if route.is_host("settings") && route.is_root() {
            return translate("settings-title");
        }
    }
    if stack.url.is_empty() && stack.title == "New Stack" {
        return translate("layout-new-stack");
    }
    stack.title.clone()
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
