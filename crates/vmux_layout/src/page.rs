#![allow(non_snake_case)]

use crate::event::{
    BOOKMARKS_EVENT, BookmarkNode, BookmarkRow, BookmarksCommandEvent, BookmarksHostEvent,
    FolderRow, HeaderCommandEvent, LAYOUT_STATE_EVENT, LayoutStateEvent, PANE_TREE_EVENT, PaneNode,
    PaneTreeEvent, RELOAD_EVENT, ReloadEvent, STACKS_EVENT, StackNode, StackRow, StacksHostEvent,
    TABS_EVENT, TabRow, TabsCommandEvent, TabsHostEvent,
};
use dioxus::prelude::*;
use vmux_core::PageIcon;
use vmux_core::event::extension::{
    EXTENSIONS_LIST_EVENT, ExtActionRequest, ExtListRequest, ExtOpenManagerRequest, ExtRow,
    ExtensionsEvent,
};
use vmux_core::event::team::{TEAM_EVENT, TeamCommandEvent, TeamEvent, TeamMemberRow};
use vmux_ui::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use vmux_ui::components::icon::Icon;
use vmux_ui::favicon::{favicon_src_for_url, host_for_favicon_fallback};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_event, use_theme};
use vmux_ui::icon::PageIconView;
use wasm_bindgen::JsCast;

#[component]
pub fn Page() -> Element {
    use_theme();

    let mut layout_state = use_signal(LayoutStateEvent::default);
    let mut layout_state_received = use_signal(|| false);
    let layout_listener =
        use_bin_event_listener::<LayoutStateEvent, _>(LAYOUT_STATE_EVENT, move |data| {
            layout_state_received.set(true);
            layout_state.set(data);
        });

    let mut stacks_state = use_signal(StacksHostEvent::default);
    let mut stacks_state_received = use_signal(|| false);
    let stacks_listener = use_bin_event_listener::<StacksHostEvent, _>(STACKS_EVENT, move |data| {
        stacks_state_received.set(true);
        stacks_state.set(data);
    });

    let mut tabs_state = use_signal(TabsHostEvent::default);
    let mut tabs_state_received = use_signal(|| false);
    let tabs_listener = use_bin_event_listener::<TabsHostEvent, _>(TABS_EVENT, move |data| {
        tabs_state_received.set(true);
        tabs_state.set(data);
    });

    let mut bookmarks_state = use_signal(BookmarksHostEvent::default);
    let _bookmarks_listener =
        use_bin_event_listener::<BookmarksHostEvent, _>(BOOKMARKS_EVENT, move |data| {
            bookmarks_state.set(data);
        });

    let mut reload_key = use_signal(|| 0u32);
    let _reload_listener = use_bin_event_listener::<ReloadEvent, _>(RELOAD_EVENT, move |_| {
        reload_key.set(reload_key() + 1);
    });

    let mut pane_tree_state = use_signal(PaneTreeEvent::default);
    let mut pane_tree_state_received = use_signal(|| false);
    let pane_tree_listener =
        use_bin_event_listener::<PaneTreeEvent, _>(PANE_TREE_EVENT, move |data| {
            pane_tree_state_received.set(true);
            pane_tree_state.set(data);
        });

    let mut spaces_state = use_signal(vmux_core::event::space::SpacesListEvent::default);
    let mut spaces_state_received = use_signal(|| false);
    let spaces_listener = use_bin_event_listener::<vmux_core::event::space::SpacesListEvent, _>(
        vmux_core::event::space::SPACES_LIST_EVENT,
        move |data| {
            spaces_state_received.set(true);
            spaces_state.set(data);
        },
    );

    let boundary_state = use_event::<crate::event::TabBoundaryEvent>(
        crate::event::TAB_BOUNDARY_EVENT,
        crate::event::TabBoundaryEvent::default,
    );

    let team_state = use_event::<TeamEvent>(TEAM_EVENT, TeamEvent::default);

    let extensions_state =
        use_event::<ExtensionsEvent>(EXTENSIONS_LIST_EVENT, ExtensionsEvent::default);
    use_effect(move || {
        let _ = try_cef_bin_emit_rkyv(&ExtListRequest);
    });

    let mut update_phase = use_signal(|| None::<UpdatePhase>);
    let _update_progress_listener = use_bin_event_listener::<crate::event::UpdateProgressEvent, _>(
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
    let _update_ready_listener = use_bin_event_listener::<crate::event::UpdateReadyEvent, _>(
        crate::event::UPDATE_READY_EVENT,
        move |evt| {
            update_phase.set(Some(UpdatePhase::Ready {
                version: evt.version,
            }))
        },
    );
    let _update_cleared_listener = use_bin_event_listener::<crate::event::UpdateClearedEvent, _>(
        crate::event::UPDATE_CLEARED_EVENT,
        move |_| update_phase.set(None),
    );

    let state = layout_state();
    let stacks = stacks_state();
    let tabs = tabs_state();
    let PaneTreeEvent { panes } = pane_tree_state();
    let active_space = spaces_state().spaces.into_iter().find(|s| s.is_active);
    let tab_boundary = boundary_state().boundary;
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
    use_effect(move || {
        if let Some(doc) = web_sys::window().and_then(|w| w.document())
            && let Some(root) = doc.document_element()
            && let Ok(html) = root.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = html
                .style()
                .set_property("--radius", &format!("{radius_px}px"));
        }
    });
    use_effect(move || {
        let PaneTreeEvent { panes } = pane_tree_state();
        let active_pane = panes.iter().find(|p| p.is_active);
        let target = active_pane
            .and_then(|p| {
                p.stacks
                    .iter()
                    .find(|s| s.is_active)
                    .map(|s| (p.id, s.stack_index))
            })
            .or_else(|| {
                panes.iter().find_map(|p| {
                    p.stacks
                        .iter()
                        .find(|s| s.is_active)
                        .map(|s| (p.id, s.stack_index))
                })
            });
        let Some((pane_id, stack_index)) = target else {
            return;
        };
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(&format!("sidesheet-stack-{pane_id}-{stack_index}")))
        {
            let opts = web_sys::ScrollIntoViewOptions::new();
            opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
            el.scroll_into_view_with_scroll_into_view_options(&opts);
        }
    });
    let side_sheet_vars = format!(
        "--vmux-side-sheet-width:{}px;--vmux-side-sheet-left:{}px;--vmux-side-sheet-top:{}px;--vmux-side-sheet-bottom:{}px;--vmux-side-sheet-pad-top:{}px;",
        state.side_sheet_width,
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
                    class: "pointer-events-auto fixed left-[var(--vmux-side-sheet-left)] top-[var(--vmux-side-sheet-top)] bottom-[var(--vmux-side-sheet-bottom)] min-h-0 overflow-hidden w-[var(--vmux-side-sheet-width)] pt-[var(--vmux-side-sheet-pad-top)]",
                    style: "{side_sheet_vars}",
                    div { class: "flex h-full min-h-0 flex-col",
                        SideSheetView {
                            panes,
                            active_space,
                            tab_boundary,
                            bookmarks: bookmarks_state(),
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
                        team: team_state().members,
                        extensions: extensions_state().extensions,
                        reload_key: reload_key(),
                        stacks_error: stacks_error.clone(),
                        tabs_error: tabs_error.clone(),
                    }
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

fn format_address(stack: &StackRow) -> String {
    if stack.url.starts_with("vmux://") || stack.url.starts_with("file:") {
        return stack.url.clone();
    }
    let host = host_for_favicon_fallback(&stack.url);
    let title = stack.title.trim();
    match (host, title.is_empty()) {
        (Some(h), false) => format!("{h} / {title}"),
        (Some(h), true) => h.to_string(),
        (None, false) => title.to_string(),
        (None, true) => stack.url.clone(),
    }
}

#[component]
fn HeaderView(
    stacks_state: StacksHostEvent,
    tabs_state: TabsHostEvent,
    bookmarks: BookmarksHostEvent,
    team: Vec<TeamMemberRow>,
    extensions: Vec<ExtRow>,
    reload_key: u32,
    stacks_error: Option<String>,
    tabs_error: Option<String>,
) -> Element {
    let StacksHostEvent {
        stacks,
        can_go_back,
        can_go_forward,
        is_zoomed: _,
    } = stacks_state;
    let TabsHostEvent { tabs } = tabs_state;
    let active_row = stacks.iter().find(|t| t.is_active).cloned();
    let active_bg_color = active_row.as_ref().and_then(|r| r.bg_color.clone());
    let active_url = active_row
        .as_ref()
        .map(|r| r.url.clone())
        .unwrap_or_default();
    let show_bookmark = !active_url.is_empty();
    let is_bookmarked = show_bookmark
        && bookmarks.roots.iter().any(|n| match n {
            BookmarkNode::Entry(b) => b.url == active_url,
            BookmarkNode::Folder(f) => f.children.iter().any(|b| b.url == active_url),
        });

    let (url_row_style, url_row_class) = url_row_cef(active_bg_color.as_deref());

    rsx! {
        div {
            class: "flex h-full min-h-0 min-w-0 flex-col text-foreground",
            div { class: "flex min-w-0 shrink-0 items-center gap-1 pl-[var(--vmux-tab-row-pad-left)] pr-2",
                if let Some(err) = tabs_error {
                    span { class: "text-ui text-destructive", "{err}" }
                } else {
                    div { class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto pl-2",
                        for tab in tabs.iter() {
                            {
                                let mut tab = tab.clone();
                                if tab.is_active {
                                    tab.bg_color = active_bg_color.clone();
                                }
                                rsx! { Tab { key: "{tab.id}", tab } }
                            }
                        }
                        NewTabButton {}
                    }
                }
            }
            div {
                class: "{url_row_class}",
                style: "{url_row_style}",
                if let Some(err) = stacks_error {
                    span { class: "text-ui text-destructive", "{err}" }
                } else {
                    NavButton { label: "Back", command: "prev_page", disabled: !can_go_back,
                        Icon { class: "h-4 w-4",
                            path { d: "M19 12H5" }
                            path { d: "M12 19l-7-7 7-7" }
                        }
                    }
                    NavButton { label: "Forward", command: "next_page", disabled: !can_go_forward,
                        Icon { class: "h-4 w-4",
                            path { d: "M5 12h14" }
                            path { d: "M12 5l7 7-7 7" }
                        }
                    }
                    NavButton { label: "Reload", command: "reload", disabled: active_row.as_ref().is_none_or(|t| t.url.is_empty()),
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
                            aria_label: "Bookmark this page",
                            title: "Bookmark this page (\u{2318}D)",
                            class: if is_bookmarked {
                                "flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-foreground transition-colors hover:bg-glass-hover"
                            } else {
                                "flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground"
                            },
                            onclick: move |_| {
                                let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
                                    command: "toggle_active".into(),
                                    uuid: None,
                                    name: None,
                                    url: None,
                                    title: None,
                                    favicon_url: None,
                                });
                            },
                            Icon { class: "h-4 w-4",
                                path {
                                    d: "M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z",
                                    fill: if is_bookmarked { "currentColor" } else { "none" },
                                }
                            }
                        }
                    }
                    TeamFacepile { members: team }
                    ExtensionBar { extensions }
                }
            }
        }
    }
}

#[component]
fn ExtensionBar(extensions: Vec<ExtRow>) -> Element {
    rsx! {
        div { class: "flex shrink-0 items-center gap-1 pl-1",
            for ext in extensions.iter().filter(|e| e.enabled && e.icon.is_some()) {
                {
                    let id = ext.id.clone();
                    let name = ext.name.clone();
                    let icon = ext.icon.clone().unwrap_or_default();
                    rsx! {
                        button {
                            key: "{ext.id}",
                            class: "flex h-7 w-7 items-center justify-center rounded-lg hover:bg-foreground/[0.08]",
                            title: "{name}",
                            onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&ExtActionRequest { id: id.clone() }); },
                            img { class: "h-4 w-4", src: "{icon}" }
                        }
                    }
                }
            }
            button {
                class: "flex h-7 w-7 items-center justify-center rounded-lg text-foreground/80 hover:bg-foreground/[0.08]",
                title: "Manage extensions",
                onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&ExtOpenManagerRequest); },
                Icon { class: "h-4 w-4",
                    path { d: "M20.5 11H19V7c0-1.1-.9-2-2-2h-4V3.5C13 2.12 11.88 1 10.5 1S8 2.12 8 3.5V5H4c-1.1 0-1.99.9-1.99 2v3.8H3.5c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7H2V20c0 1.1.9 2 2 2h3.8v-1.5c0-1.49 1.21-2.7 2.7-2.7 1.49 0 2.7 1.21 2.7 2.7V22H17c1.1 0 2-.9 2-2v-4h1.5c1.38 0 2.5-1.12 2.5-2.5S21.88 11 20.5 11z" }
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
fn HeaderAddressBar(active_row: Option<StackRow>, bg_color: Option<String>) -> Element {
    let has_content = active_row.as_ref().is_some_and(|t| !t.url.is_empty());
    let address_value = active_row.as_ref().map(format_address).unwrap_or_default();
    let placeholder = if has_content { "" } else { "New Stack" };
    let placeholder_class = if bg_color.is_some() {
        "placeholder:opacity-50"
    } else {
        "placeholder:text-muted-foreground"
    };

    rsx! {
        div {
            class: "flex h-8 min-w-0 flex-1 cursor-pointer items-center",
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&HeaderCommandEvent {
                    header_command: "focus_address_bar".to_string(),
                });
            },
            input {
                r#type: "text",
                readonly: true,
                class: "min-w-0 flex-1 cursor-pointer bg-transparent text-ui outline-none {placeholder_class}",
                value: "{address_value}",
                placeholder: "{placeholder}",
            }
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
fn NavButton(
    label: &'static str,
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
            aria_label: label,
            title: label,
            disabled,
            class,
            onclick: move |_| {
                if !disabled {
                    let _ = try_cef_bin_emit_rkyv(&HeaderCommandEvent {
                        header_command: command.to_string(),
                    });
                }
            },
            {children}
        }
    }
}

fn dir_truncate_class(title: &str) -> &'static str {
    if title.starts_with('/') || title.starts_with("~/") {
        "truncate-start"
    } else {
        "truncate"
    }
}

#[component]
fn Tab(tab: TabRow) -> Element {
    let id_switch = tab.id.clone();
    let id_close = tab.id.clone();
    let display_title = if !tab.title.is_empty() {
        tab.title.clone()
    } else if !tab.name.is_empty() {
        tab.name.clone()
    } else {
        "Tab".to_string()
    };
    let tooltip = display_title.clone();
    let is_active = tab.is_active;
    let skirt_classes = "relative \
        before:content-[''] before:absolute before:bottom-0 before:-left-2 before:h-2 before:w-2 before:pointer-events-none \
        before:[background:radial-gradient(circle_at_top_left,transparent_0,transparent_8px,var(--tab-bg)_8px)] \
        after:content-[''] after:absolute after:bottom-0 after:-right-2 after:h-2 after:w-2 after:pointer-events-none \
        after:[background:radial-gradient(circle_at_top_right,transparent_0,transparent_8px,var(--tab-bg)_8px)]";
    let tab_box_classes = "group flex h-10 w-52 min-w-52 max-w-52 basis-52 shrink-0 grow-0 -mb-[3px] pb-[3px] cursor-pointer items-center gap-2 px-3.5";

    let trunc = dir_truncate_class(&display_title);
    let (tab_style, tab_class, title_class, close_class) = if is_active {
        (
            "--tab-bg:var(--glass);".to_string(),
            format!("{skirt_classes} {tab_box_classes} glass rounded-t-md border-b-0"),
            format!("min-w-0 flex-1 {trunc} text-ui font-medium text-foreground"),
            "flex h-4 w-4 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10".to_string(),
        )
    } else {
        (
            String::new(),
            format!(
                "{tab_box_classes} rounded-md text-muted-foreground hover:bg-glass-hover hover:px-4 hover:text-foreground"
            ),
            format!("min-w-0 flex-1 {trunc} text-ui"),
            "flex h-4 w-4 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10".to_string(),
        )
    };

    let bm_url = tab.url.clone();
    let bm_title = display_title.clone();
    let bm_favicon = tab.icon.favicon_url().to_string();
    let pin_url = tab.url.clone();
    let pin_title = display_title.clone();
    let pin_favicon = tab.icon.favicon_url().to_string();
    let menu_val = use_signal(|| tab.id.clone());

    rsx! {
        ContextMenu { attributes: vec![],
            ContextMenuTrigger { attributes: vec![],
        div {
            class: "{tab_class}",
            style: "{tab_style}",
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&TabsCommandEvent {
                    command: "switch".to_string(),
                    tab_id: Some(id_switch.clone()),
                });
            },
            div {
                title: "{tooltip}",
                class: "flex min-w-0 flex-1 items-center gap-2.5 overflow-hidden",
                StackIcon {
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
                aria_label: "Close tab",
                title: "Close tab",
                class: "{close_class}",
                onmousedown: move |evt| {
                    evt.prevent_default();
                    evt.stop_propagation();
                },
                onclick: move |evt| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    let _ = try_cef_bin_emit_rkyv(&TabsCommandEvent {
                        command: "close".to_string(),
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
            ContextMenuContent { attributes: vec![],
                ContextMenuItem {
                    index: 0usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| add_to_bookmarks("add", bm_url.clone(), bm_title.clone(), bm_favicon.clone()),
                    attributes: vec![],
                    "Bookmark"
                }
                ContextMenuItem {
                    index: 1usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| add_to_bookmarks("pin_url", pin_url.clone(), pin_title.clone(), pin_favicon.clone()),
                    attributes: vec![],
                    "Pin"
                }
            }
        }
    }
}

#[component]
fn NewTabButton() -> Element {
    rsx! {
        button {
            r#type: "button",
            aria_label: "New tab",
            title: "New tab",
            class: "flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground active:bg-glass-active active:text-foreground",
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&TabsCommandEvent {
                    command: "new".to_string(),
                    tab_id: None,
                });
            },
            Icon { class: "h-3.5 w-3.5",
                path { d: "M12 5v14" }
                path { d: "M5 12h14" }
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
                    title: "Team",
                    onclick: move |_| {
                        let _ = try_cef_bin_emit_rkyv(&TeamCommandEvent {
                            command: "open".to_string(),
                            member_id: None,
                        });
                    },
                    div {
                        class: "inline-flex size-5 items-center justify-center rounded-full text-[9px] font-semibold text-white",
                        style: "background:{user.color}",
                        "{user.initials}"
                    }
                    span { class: "whitespace-nowrap text-xs font-medium text-foreground", "{user.name}" }
                }
            }
            if !agents.is_empty() {
                div { class: "flex items-center -space-x-1.5",
                    for m in agents.iter().take(max) {
                        {
                            let src = favicon_src_for_url(&m.icon, &m.url);
                            let bg = if src.is_some() { String::new() } else { format!("background:{}", m.color) };
                            let id = m.id.clone();
                            rsx! {
                                div {
                                    key: "{m.id}",
                                    title: "{m.name}",
                                    class: "relative inline-flex size-5 shrink-0 cursor-pointer transition-opacity hover:opacity-80",
                                    onclick: move |_| {
                                        let _ = try_cef_bin_emit_rkyv(&TeamCommandEvent {
                                            command: "focus".to_string(),
                                            member_id: Some(id.clone()),
                                        });
                                    },
                                    div {
                                        class: "inline-flex size-5 items-center justify-center overflow-hidden rounded-full ring-2 ring-background text-[9px] font-semibold text-white",
                                        style: "{bg}",
                                        if let Some(src) = src.as_ref() {
                                            img { class: "size-full object-cover", src: "{src}" }
                                        } else {
                                            "{m.initials}"
                                        }
                                    }
                                    if m.is_running {
                                        span { class: "absolute -bottom-0.5 -right-0.5 size-1.5 rounded-full bg-emerald-400 ring-2 ring-background" }
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
                            title: "Team",
                            onclick: move |_| {
                                let _ = try_cef_bin_emit_rkyv(&TeamCommandEvent {
                                    command: "open".to_string(),
                                    member_id: None,
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
fn SideSheetView(
    panes: Vec<PaneNode>,
    active_space: Option<vmux_core::event::space::SpaceRow>,
    tab_boundary: Option<crate::event::TabBoundary>,
    bookmarks: BookmarksHostEvent,
    pane_tree_error: Option<String>,
) -> Element {
    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col px-2 pb-3 pt-2 text-foreground",
            if let Some(space) = active_space {
                div { class: "glass mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg",
                    SideSheetSpaceRow { key: "{space.id}", space: space.clone() }
                    if let Some(b) = tab_boundary {
                        TabBoundaryPanel { boundary: b }
                    } else if !space.startup_dir.is_empty() {
                        div { class: "flex items-center gap-1.5 border-t border-foreground/10 px-2.5 py-2 text-muted-foreground",
                            Icon { class: "h-3.5 w-3.5 shrink-0",
                                path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                            }
                            span {
                                class: "min-w-0 flex-1 truncate text-xs",
                                style: "direction:rtl;",
                                title: "{space.startup_dir}",
                                bdi { style: "unicode-bidi:isolate;direction:ltr;", "{space.startup_dir}" }
                            }
                        }
                    }
                }
            }
            BookmarksSection { bookmarks }
            div { class: "flex min-h-0 flex-1 flex-col overflow-y-auto",
                if let Some(err) = pane_tree_error {
                    div { class: "flex items-center px-2 py-1",
                        span { class: "text-ui text-destructive", "{err}" }
                    }
                } else if panes.is_empty() {
                    div { class: "flex items-center px-2 py-1",
                        span { class: "text-ui text-muted-foreground", "No stacks" }
                    }
                } else {
                    for (i, pane) in panes.iter().enumerate() {
                        PaneSection { key: "{pane.id}", pane: pane.clone(), index: i }
                    }
                }
            }
        }
    }
}

/// The active tab's working directory + live git status, rendered inside the space card. Shows the
/// dir always; when it's a git repo, adds an auto-detected git row (branch, worktree, dirty/ahead).
/// Read-only — worktree lifecycle is agent-driven (no UI actions).
#[component]
fn TabBoundaryPanel(boundary: crate::event::TabBoundary) -> Element {
    let b = boundary;
    rsx! {
        div { class: "flex flex-col gap-1.5 border-t border-foreground/10 px-2.5 py-2",
            div { class: "flex items-center gap-1.5 text-muted-foreground",
                Icon { class: "h-3.5 w-3.5 shrink-0",
                    path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                }
                span {
                    class: "min-w-0 flex-1 truncate text-xs",
                    style: "direction:rtl;",
                    title: "{b.effective_dir}",
                    bdi { style: "unicode-bidi:isolate;direction:ltr;", "{b.effective_dir}" }
                }
            }
            if b.is_git_repo {
                div { class: "flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground",
                    span { class: "flex min-w-0 items-center gap-1",
                        Icon { class: "h-3 w-3 shrink-0 opacity-80",
                            path { d: "M6 3v12" }
                            path { d: "M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" }
                            path { d: "M6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" }
                            path { d: "M18 9a9 9 0 0 1-9 9" }
                        }
                        span { class: "min-w-0 truncate text-foreground/90", "{b.branch}" }
                    }
                    if b.is_worktree {
                        span { class: "shrink-0 rounded bg-foreground/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide",
                            "worktree"
                        }
                    }
                    if b.uncommitted > 0 {
                        span { class: "shrink-0 text-amber-400/90", "● {b.uncommitted}" }
                    }
                    if b.ahead > 0 {
                        span { class: "shrink-0", "↑ {b.ahead}" }
                    }
                    if b.is_worktree && !b.base_ref.is_empty() {
                        span { class: "shrink-0 opacity-60", "← {b.base_ref}" }
                    }
                }
            }
        }
    }
}

fn open_bookmark(url: String) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: "open".into(),
        url: Some(url),
        uuid: None,
        name: None,
        title: None,
        favicon_url: None,
    });
}

fn bookmark_cmd(command: &str, uuid: Option<String>) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: command.into(),
        uuid,
        name: None,
        url: None,
        title: None,
        favicon_url: None,
    });
}

fn add_to_bookmarks(command: &str, url: String, title: String, favicon_url: String) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: command.into(),
        uuid: None,
        name: None,
        url: Some(url),
        title: Some(title),
        favicon_url: Some(favicon_url),
    });
}

fn request_bookmark_menu() {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: "menu_new_folder".into(),
        uuid: None,
        name: None,
        url: None,
        title: None,
        favicon_url: None,
    });
}

fn commit_folder_rename(uuid: String, name: String) {
    let name = name.trim().to_string();
    let command = if name.is_empty() {
        "remove_folder"
    } else {
        "rename_folder"
    };
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: command.into(),
        uuid: Some(uuid),
        name: if name.is_empty() { None } else { Some(name) },
        url: None,
        title: None,
        favicon_url: None,
    });
}

#[component]
fn BookmarksSection(bookmarks: BookmarksHostEvent) -> Element {
    let BookmarksHostEvent { pins, roots } = bookmarks;

    if pins.is_empty() && roots.is_empty() {
        return rsx! {
            div {
                class: "glass relative z-30 mb-2 flex items-center justify-center rounded-lg px-2 py-4 text-ui-xs text-muted-foreground",
                oncontextmenu: move |e| {
                    e.prevent_default();
                    request_bookmark_menu();
                },
                "No pins or bookmarks"
            }
        };
    }

    rsx! {
        div {
            class: "glass relative z-30 mb-2 flex flex-col rounded-lg p-1.5",
            oncontextmenu: move |e: Event<MouseData>| {
                let data = e.data();
                let on_card = data
                    .downcast::<web_sys::MouseEvent>()
                    .map(|m| m.target() == m.current_target())
                    .unwrap_or(false);
                if on_card {
                    e.prevent_default();
                    request_bookmark_menu();
                }
            },
            if !pins.is_empty() {
                div { class: "mb-1 grid grid-cols-3 gap-2 p-1",
                    for p in pins.iter() {
                        PinTile { key: "{p.uuid}", row: p.clone() }
                    }
                }
            }
            div { class: "flex flex-col gap-1",
                for node in roots.iter() {
                    match node {
                        BookmarkNode::Folder(f) => rsx! { BookmarkFolder { key: "{f.uuid}", folder: f.clone() } },
                        BookmarkNode::Entry(b) => rsx! { BookmarkEntry { key: "{b.uuid}", row: b.clone() } },
                    }
                }
            }
        }
    }
}

#[component]
fn PinTile(row: BookmarkRow) -> Element {
    let url_open = row.url.clone();
    let uuid = row.uuid.clone();
    let menu_val = use_signal(|| row.uuid.clone());
    rsx! {
        ContextMenu { attributes: vec![],
            ContextMenuTrigger { attributes: vec![],
                div {
                    class: "flex aspect-square cursor-pointer items-center justify-center rounded-lg bg-white/5 hover:bg-white/10",
                    onclick: {
                        let u = url_open.clone();
                        move |_| open_bookmark(u.clone())
                    },
                    title: "{row.title}",
                    Favicon {
                        favicon_url: row.favicon_url.clone(),
                        url: row.url.clone(),
                        class: "h-6 w-6 shrink-0 rounded-sm object-contain".to_string(),
                        globe_class: "h-6 w-6 shrink-0 text-muted-foreground".to_string(),
                    }
                }
            }
            ContextMenuContent { attributes: vec![],
                ContextMenuItem {
                    index: 0usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: { let u = url_open.clone(); move |_: String| open_bookmark(u.clone()) },
                    attributes: vec![],
                    "Open"
                }
                ContextMenuItem {
                    index: 1usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: { let id = uuid.clone(); move |_: String| bookmark_cmd("unpin", Some(id.clone())) },
                    attributes: vec![],
                    "Unpin"
                }
            }
        }
    }
}

#[component]
fn BookmarkEntry(row: BookmarkRow) -> Element {
    let url_open = row.url.clone();
    let uuid_pin = row.uuid.clone();
    let uuid_remove = row.uuid.clone();
    let menu_val = use_signal(|| row.uuid.clone());
    let title = if row.title.is_empty() {
        row.url.clone()
    } else {
        row.title.clone()
    };
    let title_class = format!("min-w-0 flex-1 {} text-ui", dir_truncate_class(&title));
    rsx! {
        ContextMenu { attributes: vec![],
            ContextMenuTrigger { attributes: vec![],
                SheetEntryRow {
                    active: false,
                    onclick: {
                        let u = url_open.clone();
                        move |_| open_bookmark(u.clone())
                    },
                    Favicon {
                        favicon_url: row.favicon_url.clone(),
                        url: row.url.clone(),
                        class: "h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
                        globe_class: "h-4 w-4 shrink-0 text-muted-foreground".to_string(),
                    }
                    span { class: "{title_class}", "{title}" }
                }
            }
            ContextMenuContent { attributes: vec![],
                ContextMenuItem {
                    index: 0usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: { let u = url_open.clone(); move |_: String| open_bookmark(u.clone()) },
                    attributes: vec![],
                    "Open"
                }
                ContextMenuItem {
                    index: 1usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: { let id = uuid_pin.clone(); move |_: String| bookmark_cmd("pin", Some(id.clone())) },
                    attributes: vec![],
                    "Pin"
                }
                ContextMenuItem {
                    index: 2usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: { let id = uuid_remove.clone(); move |_: String| bookmark_cmd("remove", Some(id.clone())) },
                    attributes: vec![],
                    "Remove"
                }
            }
        }
    }
}

#[component]
fn BookmarkFolder(folder: FolderRow) -> Element {
    let uuid = folder.uuid.clone();
    let collapsed = folder.collapsed;
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| folder.name.clone());
    let menu_val = use_signal(|| folder.uuid.clone());

    rsx! {
        div { class: "flex flex-col gap-1",
            if editing() {
                div { class: "flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                    Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                        path { d: if collapsed { "m9 18 6-6-6-6" } else { "m6 9 6 6 6-6" } }
                    }
                    input {
                        r#type: "text",
                        class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none",
                        placeholder: "Folder name",
                        value: "{draft}",
                        autofocus: true,
                        oncontextmenu: move |e| e.prevent_default(),
                        onmounted: move |e: Event<MountedData>| {
                            if let Some(el) = e.downcast::<web_sys::Element>()
                                && let Ok(input) = el.clone().dyn_into::<web_sys::HtmlElement>()
                            {
                                let _ = input.focus();
                            }
                        },
                        oninput: move |e| draft.set(e.value()),
                        onkeydown: {
                            let id = uuid.clone();
                            move |e: Event<KeyboardData>| match e.key() {
                                Key::Enter => {
                                    e.prevent_default();
                                    editing.set(false);
                                    commit_folder_rename(id.clone(), draft());
                                }
                                Key::Escape => {
                                    e.prevent_default();
                                    editing.set(false);
                                }
                                _ => {}
                            }
                        },
                        onblur: {
                            let id = uuid.clone();
                            move |_| {
                                if editing() {
                                    editing.set(false);
                                    commit_folder_rename(id.clone(), draft());
                                }
                            }
                        },
                    }
                }
            } else {
                ContextMenu { attributes: vec![],
                    ContextMenuTrigger { attributes: vec![],
                        SheetEntryRow {
                            active: false,
                            onclick: {
                                let id = uuid.clone();
                                move |_| bookmark_cmd("toggle_folder", Some(id.clone()))
                            },
                            Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                                path { d: if collapsed { "m9 18 6-6-6-6" } else { "m6 9 6 6 6-6" } }
                            }
                            span { class: "min-w-0 flex-1 truncate text-ui font-medium text-foreground", "{folder.name}" }
                        }
                    }
                    ContextMenuContent { attributes: vec![],
                        ContextMenuItem {
                            index: 0usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let id = uuid.clone(); move |_: String| bookmark_cmd("toggle_folder", Some(id.clone())) },
                            attributes: vec![],
                            if collapsed { "Expand" } else { "Collapse" }
                        }
                        ContextMenuItem {
                            index: 1usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: {
                                let name = folder.name.clone();
                                move |_: String| {
                                    draft.set(name.clone());
                                    editing.set(true);
                                }
                            },
                            attributes: vec![],
                            "Rename Folder"
                        }
                        ContextMenuItem {
                            index: 2usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let id = uuid.clone(); move |_: String| bookmark_cmd("remove_folder", Some(id.clone())) },
                            attributes: vec![],
                            "Remove Folder"
                        }
                    }
                }
            }
            if !collapsed {
                div { class: "ml-3 flex flex-col gap-1",
                    for b in folder.children.iter() {
                        BookmarkEntry { key: "{b.uuid}", row: b.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn SideSheetSpaceRow(space: vmux_core::event::space::SpaceRow) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "group flex w-full cursor-pointer items-center gap-2 px-2 py-1.5 text-foreground hover:bg-foreground/5",
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&vmux_core::event::space::SpaceCommandEvent {
                    command: "open_page".to_string(),
                    space_id: Some(space.id.clone()),
                    name: None,
                });
            },
            Icon { class: "h-4 w-4 shrink-0",
                path { d: "M3 3h7v7H3z" }
                path { d: "M14 3h7v7h-7z" }
                path { d: "M3 14h7v7H3z" }
                path { d: "M14 14h7v7h-7z" }
            }
            span {
                class: "min-w-0 flex-1 truncate text-ui font-medium text-foreground text-left",
                "{space.name}"
            }
        }
    }
}

#[component]
fn SheetEntryRow(active: bool, onclick: EventHandler<MouseEvent>, children: Element) -> Element {
    rsx! {
        div {
            class: if active {
                "glass group flex h-9 cursor-default items-center gap-2 rounded-md px-2"
            } else {
                "group flex h-9 cursor-pointer items-center gap-2 rounded-md px-2 border border-transparent text-muted-foreground hover:bg-glass-hover hover:text-foreground"
            },
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
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

#[component]
fn PaneSection(pane: PaneNode, index: usize) -> Element {
    let label = format!("Stack {}", index + 1);
    let pane_id = pane.id;
    let any_loading = pane.stacks.iter().any(|s| s.is_loading);
    let folded_stack = pane.stacks.iter().find(|stack| stack.is_active).cloned();
    let mut folded = use_signal(|| false);

    rsx! {
        div { class: if pane.is_active && any_loading {
                "glass group mb-2 flex flex-col rounded-lg p-1.5 pane-loading-ring"
            } else if pane.is_active {
                "glass group mb-2 flex flex-col rounded-lg p-1.5 ring-2 ring-ring"
            } else {
                "glass group mb-2 flex flex-col rounded-lg p-1.5"
            },
            div {
                class: "mb-0.5 flex items-center gap-1 rounded-md px-2 py-1",
                span {
                    class: if pane.is_active {
                        "min-w-0 flex-1 text-ui font-semibold text-foreground"
                    } else {
                        "min-w-0 flex-1 text-ui font-medium text-muted-foreground"
                    },
                    "{label}"
                }
                button {
                    r#type: "button",
                    aria_label: if folded() { "Unfold stack" } else { "Fold stack" },
                    title: if folded() { "Unfold stack" } else { "Fold stack" },
                    class: if folded() {
                        "flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm bg-foreground/10 text-foreground"
                    } else {
                        "flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10 hover:text-foreground"
                    },
                    onclick: move |_| {
                        let next = !folded();
                        folded.set(next);
                    },
                    Icon { class: "h-3.5 w-3.5 pointer-events-none",
                        path { d: if folded() { "m9 18 6-6-6-6" } else { "m6 9 6 6 6-6" } }
                    }
                }
            }
            if folded() {
                if let Some(stack) = folded_stack {
                    SideSheetStackRow { stack, pane_id }
                }
            } else {
                div { class: "flex flex-col gap-1",
                    for stack in pane
                        .stacks
                        .iter()
                        .filter(|s| !(s.url.is_empty() && s.title == "New Stack"))
                    {
                        SideSheetStackRow { stack: stack.clone(), pane_id }
                    }
                    NewStackRow { pane_id }
                }
            }
        }
    }
}

#[component]
fn NewStackRow(pane_id: u64) -> Element {
    rsx! {
        SheetNewButton {
            label: "New Stack".to_string(),
            icon: rsx! {
                Icon { class: "h-4 w-4 shrink-0",
                    path { d: "M12 5v14" }
                    path { d: "M5 12h14" }
                }
            },
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&crate::event::SideSheetCommandEvent {
                    command: "new_stack".to_string(),
                    pane_id: pane_id.to_string(),
                    stack_index: 0,
                });
            },
        }
    }
}

#[component]
fn SideSheetStackRow(stack: StackNode, pane_id: u64) -> Element {
    let is_active = stack.is_active;
    let stack_index = stack.stack_index;
    let mut hovered = use_signal(|| false);

    let title_class = if is_active {
        format!(
            "min-w-0 flex-1 {} text-ui font-medium text-foreground",
            dir_truncate_class(&stack.title)
        )
    } else {
        format!(
            "min-w-0 flex-1 {} text-ui",
            dir_truncate_class(&stack.title)
        )
    };

    rsx! {
        div {
            id: "sidesheet-stack-{pane_id}-{stack_index}",
            class: if is_active {
                "glass flex h-9 cursor-default items-center gap-2 rounded-md px-2"
            } else {
                "flex h-9 cursor-pointer items-center gap-2 rounded-md px-2 border border-transparent text-muted-foreground hover:bg-glass-hover hover:text-foreground"
            },
            onmouseenter: move |_| hovered.set(true),
            onmouseleave: move |_| hovered.set(false),
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&crate::event::SideSheetCommandEvent {
                    command: "activate_stack".to_string(),
                    pane_id: pane_id.to_string(),
                    stack_index,
                });
            },
            StackIcon { icon: stack.icon.clone(), url: stack.url.clone(), title: stack.title.clone() }
            span { class: "{title_class}", "{stack.title}" }
            button {
                r#type: "button",
                aria_label: "Close stack",
                title: "Close stack",
                class: if hovered() {
                    "ml-auto flex h-6 w-6 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-100 transition-opacity focus-visible:opacity-100 hover:bg-foreground/10"
                } else {
                    "ml-auto flex h-6 w-6 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity focus-visible:opacity-100 hover:bg-foreground/10"
                },
                onmousedown: move |evt| {
                    evt.prevent_default();
                    evt.stop_propagation();
                },
                onclick: move |evt| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    let _ = try_cef_bin_emit_rkyv(&crate::event::SideSheetCommandEvent {
                        command: "close_stack".to_string(),
                        pane_id: pane_id.to_string(),
                        stack_index,
                    });
                },
                Icon { class: "h-3 w-3 pointer-events-none",
                    path { d: "M18 6 6 18" }
                    path { d: "m6 6 12 12" }
                }
            }
        }
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
fn UpdateNoticeFooter(phase: UpdatePhase) -> Element {
    let (label, version) = match &phase {
        UpdatePhase::Downloading { version, .. } => ("Downloading update", version.clone()),
        UpdatePhase::Installing { version } => ("Installing update…", version.clone()),
        UpdatePhase::Ready { version } => ("New version available", version.clone()),
    };
    rsx! {
        div {
            class: "shrink-0 mx-2 mb-2 mt-2 flex flex-col gap-2 rounded-md glass px-3 py-2 text-foreground",
            div { class: "flex items-center gap-2",
                span { class: "inline-block h-2 w-2 shrink-0 rounded-full bg-green-500" }
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
                            let _ = try_cef_bin_emit_rkyv(&crate::event::RestartRequestEvent);
                        },
                        "Restart to update"
                    }
                },
            }}
        }
    }
}

#[component]
fn UpdateProgressBar(downloaded: u64, total: u64) -> Element {
    let determinate = total > 0;
    let pct = download_pct(downloaded, total);
    rsx! {
        div { class: "h-1.5 w-full overflow-hidden rounded-full bg-foreground/10",
            if determinate {
                div {
                    class: "h-full rounded-full bg-primary transition-[width] duration-200",
                    style: "width:{pct}%",
                }
            } else {
                div { class: "h-full w-1/3 rounded-full bg-primary update-progress-indeterminate" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_pct_clamps_and_handles_zero_total() {
        assert_eq!(download_pct(0, 0), 0);
        assert_eq!(download_pct(50, 100), 50);
        assert_eq!(download_pct(250, 100), 100);
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
