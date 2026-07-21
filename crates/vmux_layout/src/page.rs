#![allow(non_snake_case)]

use crate::event::{
    BOOKMARKS_EVENT, BookmarkContextMenuEvent, BookmarkNode, BookmarkRow, BookmarkTextInputEvent,
    BookmarksCommandEvent, BookmarksHostEvent, FolderRow, HeaderCommandEvent, LAYOUT_STATE_EVENT,
    LayoutStateEvent, PANE_TREE_EVENT, PaneNode, PaneTreeEvent, RELOAD_EVENT, REMOTE_STATE_EVENT,
    ReloadEvent, RemoteCommandEvent, RemotePhase, RemoteStateEvent, STACKS_EVENT, StackNode,
    StackRow, StacksHostEvent, TABS_EVENT, TabRow, TabsCommandEvent, TabsHostEvent,
};
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_core::event::extension::{
    EXTENSIONS_LIST_EVENT, ExtActionRequest, ExtListRequest, ExtOpenManagerRequest, ExtRow,
    ExtensionsEvent,
};
use vmux_core::event::team::{TEAM_EVENT, TeamCommandEvent, TeamEvent, TeamMemberRow};
use vmux_core::knowledge::{KNOWLEDGE_TREE_EVENT, KnowledgeEntry, KnowledgeTreeEvent};
use vmux_core::{PageIcon, PageMetadata};
use vmux_ui::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use vmux_ui::components::icon::Icon;
use vmux_ui::favicon::{favicon_src_for_url, host_for_favicon_fallback};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_event, use_theme};
use vmux_ui::icon::PageIconView;
use wasm_bindgen::{JsCast, closure::Closure};

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

    let mut knowledge_state = use_signal(KnowledgeTreeEvent::default);
    let mut knowledge_state_received = use_signal(|| false);
    let _knowledge_listener =
        use_bin_event_listener::<KnowledgeTreeEvent, _>(KNOWLEDGE_TREE_EVENT, move |data| {
            knowledge_state_received.set(true);
            knowledge_state.set(data);
        });

    let team_state = use_event::<TeamEvent>(TEAM_EVENT, TeamEvent::default);
    let remote_state = use_event::<RemoteStateEvent>(REMOTE_STATE_EVENT, RemoteStateEvent::default);

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
    let mut last_scrolled_stack = use_signal(|| None::<(u64, u32)>);
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
        if !layout_state().side_sheet_open {
            return;
        }
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
        if last_scrolled_stack() == Some((pane_id, stack_index)) {
            return;
        }
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(&format!("sidesheet-stack-{pane_id}-{stack_index}")))
        {
            let opts = web_sys::ScrollIntoViewOptions::new();
            opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
            el.scroll_into_view_with_scroll_into_view_options(&opts);
            last_scrolled_stack.set(Some((pane_id, stack_index)));
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
                    id: "vmux-side-sheet",
                    class: "pointer-events-auto fixed left-[var(--vmux-side-sheet-left)] top-[var(--vmux-side-sheet-top)] bottom-[var(--vmux-side-sheet-bottom)] min-h-0 overflow-visible w-[var(--vmux-side-sheet-width)] pt-[var(--vmux-side-sheet-pad-top)]",
                    style: "{side_sheet_vars}",
                    div { class: "flex h-full min-h-0 flex-col",
                        SideSheetView {
                            panes,
                            active_space,
                            tab_boundary,
                            remote: remote_state(),
                            bookmarks: bookmarks_state(),
                            knowledge: knowledge_state(),
                            knowledge_loaded: knowledge_state_received(),
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
                            aria_label: if is_bookmarked { "Remove bookmark" } else { "Bookmark this page" },
                            title: if is_bookmarked { "Remove bookmark (\u{2318}D)" } else { "Bookmark this page (\u{2318}D)" },
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
                                    metadata: None,
                                    folder: None,
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
                            aria_label: if is_pinned { "Unpin this page" } else { "Pin this page" },
                            title: if is_pinned { "Unpin this page" } else { "Pin this page" },
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
                    on_select: move |_: String| add_to_bookmarks("add", bookmark_metadata.clone(), None),
                    attributes: vec![],
                    "Bookmark"
                }
                ContextMenuItem {
                    index: 1usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| add_to_bookmarks("pin_url", pin_metadata.clone(), None),
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
    remote: RemoteStateEvent,
    bookmarks: BookmarksHostEvent,
    knowledge: KnowledgeTreeEvent,
    knowledge_loaded: bool,
    pane_tree_error: Option<String>,
) -> Element {
    let active_page = panes
        .iter()
        .find(|pane| pane.is_active)
        .and_then(|pane| pane.stacks.iter().find(|stack| stack.is_active))
        .filter(|stack| !stack.url.is_empty())
        .cloned();
    let folders = bookmark_folder_choices(&bookmarks.roots);
    let initial_folders = folders.clone();
    let mut folder_context = use_signal(|| initial_folders);
    let drag_state = use_signal(|| None::<BookmarkDragState>);
    use_context_provider(|| folder_context);
    use_context_provider(|| drag_state);
    use_effect(move || folder_context.set(folders.clone()));
    use_drop(move || {
        remove_bookmark_drag_ghost();
        set_bookmark_context_menu_active(false);
    });
    let active_pane_id = panes
        .iter()
        .find(|pane| pane.is_active)
        .or_else(|| panes.first())
        .map(|pane| pane.id);
    rsx! {
        div {
            class: "flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto px-2 pb-3 pt-2 text-foreground",
            style: "scrollbar-gutter:stable;",
            onpointermove: move |event| update_bookmark_drag(drag_state, &event),
            onpointerup: move |event| end_bookmark_drag(drag_state, &event),
            onpointercancel: move |event| cancel_bookmark_drag(drag_state, &event),
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
                    RemotePanel { remote: remote.clone() }
                }
            }
            BookmarksSection { bookmarks: bookmarks.clone(), active_page }
            if let Some(pane_id) = active_pane_id {
                KnowledgeCard { pane_id, knowledge, loaded: knowledge_loaded }
            }
            if let Some(err) = pane_tree_error {
                div { class: "flex shrink-0 items-center px-2 py-1",
                    span { class: "text-ui text-destructive", "{err}" }
                }
            } else if panes.is_empty() {
                div { class: "flex shrink-0 items-center px-2 py-1",
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

#[component]
fn RemotePanel(remote: RemoteStateEvent) -> Element {
    let mut show_pairing = use_signal(|| !remote.paired);
    let mut copied = use_signal(|| false);
    let paired = remote.paired;
    let phase = remote.phase;
    use_effect(move || {
        if !paired {
            show_pairing.set(true);
        } else if phase != RemotePhase::Enabled {
            show_pairing.set(false);
        }
    });
    let active = remote.phase == RemotePhase::Enabled;
    let transitioning = remote.phase == RemotePhase::Starting;
    let status = match remote.phase {
        RemotePhase::Disabled => "Off",
        RemotePhase::Starting if remote.enabled => "Starting…",
        RemotePhase::Starting => "Stopping…",
        RemotePhase::Enabled => "On",
        RemotePhase::Error => "Needs attention",
    };
    let qr = if active && show_pairing() && !remote.pairing_deep_link.is_empty() {
        pairing_qr_svg(&remote.pairing_deep_link)
    } else {
        None
    };
    let pairing_url = remote.pairing_url.clone();
    rsx! {
        div {
            class: if remote.enabled {
                "border-t border-emerald-400/30 bg-emerald-500/10 px-2.5 py-2.5"
            } else {
                "border-t border-foreground/10 px-2.5 py-2.5"
            },
            div { class: "flex items-center gap-2",
                div {
                    class: if remote.enabled {
                        "flex size-7 shrink-0 items-center justify-center rounded-md bg-emerald-400/15 text-emerald-400"
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
                    div { class: "flex items-center gap-1.5",
                        span { class: "text-ui font-semibold", "Remote" }
                        if active {
                            span { class: "inline-flex items-center gap-1 rounded-full bg-emerald-400/15 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-emerald-400",
                                span { class: "size-1.5 rounded-full bg-emerald-400" }
                                "Live"
                            }
                        }
                    }
                    div {
                        class: if remote.phase == RemotePhase::Error {
                            "mt-0.5 truncate text-[10px] text-destructive"
                        } else if remote.enabled {
                            "mt-0.5 text-[10px] text-emerald-400/80"
                        } else {
                            "mt-0.5 text-[10px] text-muted-foreground"
                        },
                        "{status}"
                    }
                }
                button {
                    r#type: "button",
                    class: if remote.enabled {
                        "relative h-5 w-9 shrink-0 rounded-full bg-emerald-400 transition-colors"
                    } else {
                        "relative h-5 w-9 shrink-0 rounded-full bg-foreground/15 transition-colors"
                    },
                    aria_label: "Toggle Remote",
                    aria_pressed: remote.enabled,
                    onclick: move |_| {
                        let _ = try_cef_bin_emit_rkyv(&RemoteCommandEvent {
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
                            let _ = try_cef_bin_emit_rkyv(&RemoteCommandEvent {
                                enabled: remote.enabled,
                            });
                        },
                        "Retry"
                    }
                }
            } else if transitioning {
                div { class: "mt-2 h-1 overflow-hidden rounded-full bg-foreground/10",
                    div { class: "h-full w-full rounded-full bg-emerald-400" }
                }
            } else if active {
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
                            copy_to_clipboard(&pairing_url);
                            copied.set(true);
                        },
                        if copied() { "Copied" } else { "Copy" }
                    }
                }
                if let Some(svg) = qr {
                    div { class: "mt-2 flex flex-col items-center rounded-lg bg-white p-2.5 text-zinc-950",
                        div { class: "overflow-hidden rounded-sm", dangerous_inner_html: "{svg}" }
                        div { class: "mt-1.5 text-center text-[10px] font-semibold", "Scan with your phone" }
                        div { class: "mt-0.5 text-center text-[9px] text-zinc-500", "Opens Vmux Remote and pairs automatically" }
                    }
                    div { class: "mt-1.5 text-[9px] leading-4 text-muted-foreground",
                        "Or paste the URL above into the mobile app."
                    }
                } else if remote.paired {
                    div { class: "mt-2 flex items-center gap-2",
                        div { class: "flex min-w-0 flex-1 items-center gap-1.5 text-[10px] text-emerald-400",
                            span { class: "size-1.5 rounded-full bg-emerald-400" }
                            "Phone paired"
                        }
                        button {
                            r#type: "button",
                            class: "text-[10px] font-semibold text-foreground hover:opacity-70",
                            onclick: move |_| show_pairing.set(true),
                            "Pair another"
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

fn copy_to_clipboard(value: &str) {
    let Ok(value) = serde_json::to_string(value) else {
        return;
    };
    let _ = document::eval(&format!("navigator.clipboard.writeText({value});"));
}

#[derive(Clone, PartialEq)]
struct BookmarkFolderChoice {
    uuid: String,
    label: String,
    ancestors: Vec<String>,
}

#[derive(Clone, PartialEq)]
enum BookmarkDragItem {
    Page { metadata: PageMetadata },
    Bookmark { uuid: String },
    Pin { uuid: String },
    Folder { uuid: String },
}

#[derive(Clone, PartialEq)]
enum BookmarkDropTarget {
    Root,
    Folder(String),
}

#[derive(Clone, PartialEq)]
struct BookmarkDragState {
    item: BookmarkDragItem,
    start_x: f64,
    start_y: f64,
    ghost_offset_x: f64,
    ghost_offset_y: f64,
    active: bool,
    target: Option<BookmarkDropTarget>,
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

fn open_knowledge_path(pane_id: u64, path: String) {
    let _ = try_cef_bin_emit_rkyv(&crate::event::SideSheetCommandEvent {
        command: "open_knowledge_path".to_string(),
        pane_id: pane_id.to_string(),
        stack_index: 0,
        path,
    });
}

fn compact_knowledge_path(path: &str) -> String {
    path.rfind("/.vmux/")
        .map(|index| format!("~{}", &path[index..]))
        .unwrap_or_else(|| path.to_string())
}

#[component]
fn KnowledgeCard(pane_id: u64, knowledge: KnowledgeTreeEvent, loaded: bool) -> Element {
    let mut folded = use_signal(|| false);
    let root = knowledge.root.clone();
    let landing_path = knowledge
        .entries
        .iter()
        .find(|entry| {
            !entry.is_directory
                && entry.parent == knowledge.root
                && entry.name.eq_ignore_ascii_case("welcome.md")
        })
        .map(|entry| entry.path.clone())
        .unwrap_or_else(|| root.clone());
    let root_title = compact_knowledge_path(&root);
    let root_action_title = if landing_path != root {
        "Open Welcome to Knowledge".to_string()
    } else if root.is_empty() {
        "Knowledge".to_string()
    } else {
        format!("Open {root}")
    };
    rsx! {
        div { class: "glass group mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg",
            div { class: "flex items-center transition-colors hover:bg-glass-hover",
                button {
                    r#type: "button",
                    disabled: !loaded || root.is_empty(),
                    title: "{root_action_title}",
                    class: "flex min-w-0 flex-1 items-center gap-2 px-2.5 py-2 text-left enabled:cursor-pointer disabled:cursor-default",
                    onclick: move |_| open_knowledge_path(pane_id, landing_path.clone()),
                    div { class: "grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-foreground/[0.07] text-foreground ring-1 ring-inset ring-foreground/10",
                        Icon { class: "h-3.5 w-3.5",
                            path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                        }
                    }
                    div { class: "min-w-0 flex-1",
                        div { class: "text-ui font-semibold text-foreground", "Knowledge" }
                        div { class: "truncate text-[10px] text-muted-foreground", "{root_title}" }
                    }
                }
                button {
                    r#type: "button",
                    aria_label: if folded() { "Expand knowledge" } else { "Collapse knowledge" },
                    title: if folded() { "Expand knowledge" } else { "Collapse knowledge" },
                    class: if folded() {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm bg-foreground/10 text-foreground"
                    } else {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10 hover:text-foreground"
                    },
                    onclick: move |_| folded.set(!folded()),
                    Icon { class: "h-3.5 w-3.5 pointer-events-none",
                        path { d: if folded() { "m9 18 6-6-6-6" } else { "m6 9 6 6 6-6" } }
                    }
                }
            }
            div { class: if folded() {
                    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                } else {
                    "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                },
                div { class: "overflow-hidden",
                    div { class: "border-t border-foreground/10 p-1.5",
                        if !loaded {
                            div { class: "px-2 py-2 text-ui-xs text-muted-foreground", "Loading…" }
                        } else if !knowledge.error.is_empty() {
                            div { class: "px-2 py-2 text-ui-xs text-destructive", "{knowledge.error}" }
                        } else if knowledge.entries.is_empty() {
                            div { class: "px-2 py-2 text-ui-xs text-muted-foreground", "No Markdown files" }
                        } else {
                            div { class: "flex flex-col gap-0.5",
                                for entry in knowledge.entries.iter().filter(|entry| entry.parent == knowledge.root) {
                                    KnowledgeEntryRow {
                                        key: "{entry.path}",
                                        entry: entry.clone(),
                                        entries: knowledge.entries.clone(),
                                        pane_id,
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

#[component]
fn KnowledgeEntryRow(entry: KnowledgeEntry, entries: Vec<KnowledgeEntry>, pane_id: u64) -> Element {
    let mut expanded = use_signal(|| false);
    if entry.is_directory {
        let has_children = entries.iter().any(|child| child.parent == entry.path);
        rsx! {
            div { class: "flex flex-col gap-0.5",
                button {
                    r#type: "button",
                    title: "{entry.path}",
                    class: "flex h-8 cursor-pointer items-center gap-1.5 rounded-md px-1.5 text-left text-muted-foreground hover:bg-glass-hover hover:text-foreground",
                    onclick: move |_| expanded.set(!expanded()),
                    Icon { class: "h-3 w-3 shrink-0",
                        path { d: if expanded() { "m6 9 6 6 6-6" } else { "m9 18 6-6-6-6" } }
                    }
                    Icon { class: "h-3.5 w-3.5 shrink-0",
                        path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                    }
                    span { class: "min-w-0 flex-1 truncate text-ui font-medium", "{entry.name}" }
                }
                if expanded() {
                    div { class: "ml-3 flex flex-col gap-0.5 border-l border-foreground/10 pl-1.5",
                        if has_children {
                            for child in entries.iter().filter(|child| child.parent == entry.path) {
                                KnowledgeEntryRow {
                                    key: "{child.path}",
                                    entry: child.clone(),
                                    entries: entries.clone(),
                                    pane_id,
                                }
                            }
                        } else {
                            div { class: "px-2 py-1.5 text-ui-xs text-muted-foreground", "Empty folder" }
                        }
                    }
                }
            }
        }
    } else {
        let path = entry.path.clone();
        let title = if entry.title.is_empty() {
            entry.name.clone()
        } else {
            entry.title.clone()
        };
        rsx! {
            button {
                r#type: "button",
                title: "{entry.path}",
                class: "flex h-8 cursor-pointer items-center gap-1.5 rounded-md px-1.5 pl-6 text-left text-muted-foreground hover:bg-glass-hover hover:text-foreground",
                onclick: move |_| open_knowledge_path(pane_id, path.clone()),
                Icon { class: "h-3.5 w-3.5 shrink-0",
                    path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" }
                    path { d: "M14 2v6h6" }
                }
                span { class: "min-w-0 flex-1 truncate text-ui", "{title}" }
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
        metadata: None,
        folder: None,
    });
}

fn bookmark_cmd(command: &str, uuid: Option<String>) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: command.into(),
        uuid,
        name: None,
        url: None,
        metadata: None,
        folder: None,
    });
}

fn add_to_bookmarks(command: &str, metadata: PageMetadata, folder: Option<String>) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: command.into(),
        uuid: None,
        name: None,
        url: None,
        metadata: Some(metadata),
        folder,
    });
}

fn move_bookmark(uuid: String, folder: Option<String>) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: "move".into(),
        uuid: Some(uuid),
        name: None,
        url: None,
        metadata: None,
        folder,
    });
}

fn move_pin(uuid: String, folder: Option<String>) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: "move_pin".into(),
        uuid: Some(uuid),
        name: None,
        url: None,
        metadata: None,
        folder,
    });
}

fn move_bookmark_folder(uuid: String, folder: Option<String>) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: "move_folder".into(),
        uuid: Some(uuid),
        name: None,
        url: None,
        metadata: None,
        folder,
    });
}

fn commit_bookmark_rename(uuid: String, name: String) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: "rename".into(),
        uuid: Some(uuid),
        name: Some(name),
        url: None,
        metadata: None,
        folder: None,
    });
}

fn create_bookmark_folder(name: String, parent: Option<String>) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: "new_folder".into(),
        uuid: None,
        name: Some(name),
        url: None,
        metadata: None,
        folder: parent,
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
    set_bookmark_pointer_capture(event, true);
    let coordinates = event.client_coordinates();
    let rect = bookmark_drag_source(event).map(|source| source.get_bounding_client_rect());
    state.set(Some(BookmarkDragState {
        item,
        start_x: coordinates.x,
        start_y: coordinates.y,
        ghost_offset_x: rect
            .as_ref()
            .map(|rect| coordinates.x - rect.left())
            .unwrap_or(12.0),
        ghost_offset_y: rect
            .as_ref()
            .map(|rect| coordinates.y - rect.top())
            .unwrap_or(12.0),
        active: false,
        target: None,
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
    let target = bookmark_drop_target_at(event);
    if !drag.active {
        set_bookmark_context_menu_active(true);
        create_bookmark_drag_ghost(event);
    }
    move_bookmark_drag_ghost(
        coordinates.x - drag.ghost_offset_x,
        coordinates.y - drag.ghost_offset_y,
    );
    if !drag.active || drag.target != target {
        drag.active = true;
        drag.target = target;
        state.set(Some(drag));
    }
}

fn perform_bookmark_drop(item: BookmarkDragItem, target: BookmarkDropTarget) {
    let folder = match target {
        BookmarkDropTarget::Root => None,
        BookmarkDropTarget::Folder(uuid) => Some(uuid),
    };
    match item {
        BookmarkDragItem::Page { metadata } => add_to_bookmarks("add", metadata, folder),
        BookmarkDragItem::Bookmark { uuid, .. } => move_bookmark(uuid, folder),
        BookmarkDragItem::Pin { uuid } => move_pin(uuid, folder),
        BookmarkDragItem::Folder { uuid, .. } => {
            if folder.as_deref() != Some(uuid.as_str()) {
                move_bookmark_folder(uuid, folder);
            }
        }
    }
}

fn clear_bookmark_drag_after_click(mut state: Signal<Option<BookmarkDragState>>) {
    let Some(window) = web_sys::window() else {
        state.set(None);
        return;
    };
    let callback = Closure::once(move || state.set(None));
    match window.request_animation_frame(callback.as_ref().unchecked_ref()) {
        Ok(_) => callback.forget(),
        Err(_) => state.set(None),
    }
}

fn end_bookmark_drag(mut state: Signal<Option<BookmarkDragState>>, event: &Event<PointerData>) {
    set_bookmark_pointer_capture(event, false);
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
    let target = bookmark_drop_target_at(event);
    event.prevent_default();
    event.stop_propagation();
    drag.active = true;
    drag.target = target.clone();
    remove_bookmark_drag_ghost();
    set_bookmark_context_menu_active(false);
    if let Some(target) = target {
        perform_bookmark_drop(drag.item.clone(), target);
    }
    state.set(Some(drag));
    clear_bookmark_drag_after_click(state);
}

fn cancel_bookmark_drag(mut state: Signal<Option<BookmarkDragState>>, event: &Event<PointerData>) {
    set_bookmark_pointer_capture(event, false);
    remove_bookmark_drag_ghost();
    set_bookmark_context_menu_active(false);
    state.set(None);
}

const BOOKMARK_DRAG_GHOST_ID: &str = "vmux-bookmark-drag-ghost";

fn bookmark_drag_source(event: &Event<PointerData>) -> Option<web_sys::Element> {
    let data = event.data();
    let pointer = data.downcast::<web_sys::PointerEvent>()?;
    let target = pointer.target()?.dyn_into::<web_sys::Element>().ok()?;
    target.closest("[data-bookmark-drag-source]").ok().flatten()
}

fn set_bookmark_pointer_capture(event: &Event<PointerData>, capture: bool) {
    let data = event.data();
    let Some(pointer) = data.downcast::<web_sys::PointerEvent>() else {
        return;
    };
    let Some(element) = bookmark_drag_source(event) else {
        return;
    };
    if capture {
        let _ = element.set_pointer_capture(pointer.pointer_id());
    } else {
        let _ = element.release_pointer_capture(pointer.pointer_id());
    }
}

fn create_bookmark_drag_ghost(event: &Event<PointerData>) {
    remove_bookmark_drag_ghost();
    let Some(source) = bookmark_drag_source(event) else {
        return;
    };
    let Ok(node) = source.clone_node_with_deep(true) else {
        return;
    };
    let Ok(ghost) = node.dyn_into::<web_sys::HtmlElement>() else {
        return;
    };
    let rect = source.get_bounding_client_rect();
    ghost.set_id(BOOKMARK_DRAG_GHOST_ID);
    let _ = ghost.set_attribute("aria-hidden", "true");
    let style = ghost.style();
    let _ = style.set_property("position", "fixed");
    let _ = style.set_property("z-index", "2000");
    let _ = style.set_property("pointer-events", "none");
    let _ = style.set_property("width", &format!("{}px", rect.width()));
    let _ = style.set_property("height", &format!("{}px", rect.height()));
    let _ = style.set_property("opacity", "0.92");
    let _ = style.set_property("margin", "0");
    let Some(body) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
    else {
        return;
    };
    let _ = body.append_child(&ghost);
}

fn move_bookmark_drag_ghost(left: f64, top: f64) {
    let Some(ghost) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(BOOKMARK_DRAG_GHOST_ID))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    let style = ghost.style();
    let _ = style.set_property("left", &format!("{left}px"));
    let _ = style.set_property("top", &format!("{top}px"));
}

fn remove_bookmark_drag_ghost() {
    if let Some(ghost) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(BOOKMARK_DRAG_GHOST_ID))
    {
        ghost.remove();
    }
}

fn bookmark_drop_target_at(event: &Event<PointerData>) -> Option<BookmarkDropTarget> {
    let coordinates = event.client_coordinates();
    let document = web_sys::window()?.document()?;
    let element = document.element_from_point(coordinates.x as f32, coordinates.y as f32)?;
    let target = element.closest("[data-bookmark-drop]").ok().flatten()?;
    match target.get_attribute("data-bookmark-drop").as_deref() {
        Some("root") => Some(BookmarkDropTarget::Root),
        Some(uuid) if !uuid.is_empty() => Some(BookmarkDropTarget::Folder(uuid.to_string())),
        _ => None,
    }
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
    let _ = try_cef_bin_emit_rkyv(&BookmarkTextInputEvent { active });
}

fn set_bookmark_context_menu_active(active: bool) {
    let _ = try_cef_bin_emit_rkyv(&BookmarkContextMenuEvent { active });
}

#[component]
fn LayoutContextMenu(children: Element) -> Element {
    rsx! {
        ContextMenu {
            attributes: vec![],
            on_open_change: set_bookmark_context_menu_active,
            {children}
        }
    }
}

fn begin_inline_rename(mut editing: Signal<bool>, mut draft: Signal<String>, name: String) {
    draft.set(name);
    let Some(window) = web_sys::window() else {
        editing.set(true);
        return;
    };
    let callback = Closure::once(move || editing.set(true));
    match window.request_animation_frame(callback.as_ref().unchecked_ref()) {
        Ok(_) => callback.forget(),
        Err(_) => editing.set(true),
    }
}

fn focus_and_select_inline_rename(event: Event<MountedData>) {
    if let Some(element) = event.downcast::<web_sys::Element>()
        && let Ok(input) = element.clone().dyn_into::<web_sys::HtmlInputElement>()
    {
        let _ = input.focus();
        input.select();
    }
}

#[component]
fn BookmarkNameInput(
    draft: Signal<String>,
    class: String,
    placeholder: String,
    on_commit: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut draft = draft;
    let mut finished = use_signal(|| false);
    use_drop(move || set_bookmark_text_input_active(false));

    rsx! {
        input {
            r#type: "text",
            class,
            placeholder,
            value: "{draft}",
            autofocus: true,
            oncontextmenu: move |event| event.prevent_default(),
            onmounted: move |event| {
                set_bookmark_text_input_active(true);
                focus_and_select_inline_rename(event);
            },
            oninput: move |event| draft.set(event.value()),
            onkeydown: move |event: Event<KeyboardData>| match event.key() {
                Key::Enter => {
                    event.prevent_default();
                    if !finished() {
                        finished.set(true);
                        set_bookmark_text_input_active(false);
                        on_commit.call(draft());
                    }
                }
                Key::Escape => {
                    event.prevent_default();
                    if !finished() {
                        finished.set(true);
                        set_bookmark_text_input_active(false);
                        on_cancel.call(());
                    }
                }
                _ => {}
            },
            onblur: move |_| {
                if !finished() {
                    finished.set(true);
                    set_bookmark_text_input_active(false);
                    on_commit.call(draft());
                }
            },
        }
    }
}

fn begin_new_folder(mut creating: Signal<bool>, mut draft: Signal<String>) {
    draft.set("New Folder".to_string());
    creating.set(true);
}

#[component]
fn SideSheetContextMenuContent(children: Element) -> Element {
    rsx! {
        ContextMenuContent { attributes: vec![], {children} }
    }
}

fn request_bookmark_menu() {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: "menu_new_folder".into(),
        uuid: None,
        name: None,
        url: None,
        metadata: None,
        folder: None,
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
        metadata: None,
        folder: None,
    });
}

#[component]
fn BookmarksSection(bookmarks: BookmarksHostEvent, active_page: Option<StackNode>) -> Element {
    let BookmarksHostEvent { pins, roots } = bookmarks;
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let mut folded = use_signal(|| false);
    let mut creating_folder = use_signal(|| false);
    let new_folder_draft = use_signal(|| "New Folder".to_string());
    let folders = bookmark_folder_choices(&roots);
    let folder_rows = bookmark_folder_rows(&roots);
    let root_targeted = bookmark_drop_targeted(drag_state, &BookmarkDropTarget::Root);
    let root_drop_label = drag_state()
        .filter(|drag| drag.active)
        .map(|drag| match drag.item {
            BookmarkDragItem::Page { .. } => "Add to Bookmarks",
            BookmarkDragItem::Bookmark { .. }
            | BookmarkDragItem::Pin { .. }
            | BookmarkDragItem::Folder { .. } => "Move to Bookmarks",
        });

    rsx! {
        div {
            "data-bookmark-drop": "root",
            class: "glass group relative z-30 mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg",
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
            div {
                "data-bookmark-drop": "root",
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
                        span { class: "min-w-0 flex-1 text-ui font-semibold text-foreground", "Bookmarks" }
                        button {
                            r#type: "button",
                            aria_label: "New bookmark folder",
                            title: "New Folder",
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
                    aria_label: if folded() { "Expand bookmarks" } else { "Collapse bookmarks" },
                    title: if folded() { "Expand bookmarks" } else { "Collapse bookmarks" },
                    class: if folded() {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm bg-foreground/10 text-foreground"
                    } else {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10 hover:text-foreground"
                    },
                    onclick: move |_| folded.set(!folded()),
                    Icon { class: "h-3.5 w-3.5 pointer-events-none",
                        path { d: if folded() { "m9 18 6-6-6-6" } else { "m6 9 6 6 6-6" } }
                    }
                }
            }
            div { class: if folded() {
                    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                } else {
                    "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                },
                div { class: "overflow-hidden",
                    div { class: "border-t border-foreground/10 p-1.5",
                        if !pins.is_empty() {
                            div {
                                "data-bookmark-drop": "",
                                class: "mb-1 grid grid-cols-4 gap-1.5 p-1",
                                for p in pins.iter() {
                                    PinTile { key: "{p.uuid}", row: p.clone() }
                                }
                            }
                        }
                        if creating_folder() {
                            div { class: "flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                                Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                                    path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                }
                                BookmarkNameInput {
                                    draft: new_folder_draft,
                                    class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none".to_string(),
                                    placeholder: "Folder name".to_string(),
                                    on_commit: move |name| {
                                        creating_folder.set(false);
                                        create_bookmark_folder(name, None);
                                    },
                                    on_cancel: move |_| creating_folder.set(false),
                                }
                            }
                        }
                        if pins.is_empty() && roots.is_empty() && !creating_folder() {
                            div { class: "px-2 py-2 text-ui-xs text-muted-foreground", "No pins or bookmarks" }
                        } else {
                            div { class: "flex flex-col gap-1",
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

#[component]
fn PinTile(row: BookmarkRow) -> Element {
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let url_open = row.metadata.url.clone();
    let uuid_unpin = row.uuid.clone();
    let menu_val = use_signal(|| row.uuid.clone());
    let drag_item = BookmarkDragItem::Pin {
        uuid: row.uuid.clone(),
    };
    rsx! {
        LayoutContextMenu {
            ContextMenuTrigger { attributes: vec![],
                div {
                    "data-bookmark-drag-source": "true",
                    onpointerdown: {
                        let item = drag_item.clone();
                        move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                    },
                    class: "flex aspect-square cursor-pointer items-center justify-center rounded-md bg-white/5 hover:bg-white/10",
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
                    PageIconView {
                        icon: row.metadata.icon.clone(),
                        url: row.metadata.url.clone(),
                        img_class: "h-5 w-5 shrink-0 rounded-sm object-contain".to_string(),
                        icon_class: "h-5 w-5 shrink-0 text-muted-foreground".to_string(),
                    }
                }
            }
            SideSheetContextMenuContent {
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
                    on_select: { let id = uuid_unpin.clone(); move |_: String| bookmark_cmd("unpin", Some(id.clone())) },
                    attributes: vec![],
                    "Unpin"
                }
                if row.bookmarked {
                    ContextMenuItem {
                        index: 2usize,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: { let id = row.uuid.clone(); move |_: String| bookmark_cmd("remove", Some(id.clone())) },
                        attributes: vec![],
                        "Remove Bookmark"
                    }
                }
            }
        }
    }
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
    let mut move_targets: Vec<(Option<String>, String)> = Vec::new();
    if folder_uuid.is_some() {
        move_targets.push((None, "Move to Bookmarks".to_string()));
    }
    move_targets.extend(
        folders
            .iter()
            .filter(|folder| Some(folder.uuid.as_str()) != folder_uuid.as_deref())
            .map(|folder| {
                (
                    Some(folder.uuid.clone()),
                    format!("Move to {}", folder.label),
                )
            }),
    );
    let remove_index = 3 + move_targets.len();
    let title_class = format!("min-w-0 flex-1 {} text-ui", dir_truncate_class(&title));
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
                BookmarkNameInput {
                    draft,
                    class: "min-w-0 flex-1 bg-transparent text-ui text-foreground outline-none".to_string(),
                    placeholder: String::new(),
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
            LayoutContextMenu {
                ContextMenuTrigger {
                    attributes: vec![],
                    div {
                        "data-bookmark-drag-source": "true",
                        onpointerdown: {
                            let item = drag_item.clone();
                            move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                        },
                        SheetEntryRow {
                            active: false,
                            onclick: {
                                let u = url_open.clone();
                                move |event: MouseEvent| {
                                    if bookmark_drag_blocks_click(drag_state) {
                                        event.prevent_default();
                                        event.stop_propagation();
                                        return;
                                    }
                                    open_bookmark(u.clone());
                                }
                            },
                            PageIconView {
                                icon: row.metadata.icon.clone(),
                                url: row.metadata.url.clone(),
                                img_class: "h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
                                icon_class: "h-4 w-4 shrink-0 text-muted-foreground".to_string(),
                            }
                            span { class: "{title_class}", "{title}" }
                        }
                    }
                }
                SideSheetContextMenuContent {
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
                        on_select: {
                            let name = title.clone();
                            move |_: String| begin_inline_rename(editing, draft, name.clone())
                        },
                        attributes: vec![],
                        "Rename"
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
                        if row.pinned { "Unpin" } else { "Pin" }
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
                        "Remove"
                    }
                }
            }
        }
    }
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
    let child_draft = use_signal(|| "New Folder".to_string());
    let menu_val = use_signal(|| folder.uuid.clone());
    let new_folder_uuid = uuid.clone();
    let mut move_targets = Vec::new();
    if parent_uuid.is_some() {
        move_targets.push((None, "Move to Bookmarks".to_string()));
    }
    move_targets.extend(
        folders
            .iter()
            .filter(|target| target.uuid != folder.uuid && !target.ancestors.contains(&folder.uuid))
            .map(|target| {
                (
                    Some(target.uuid.clone()),
                    format!("Move to {}", target.label),
                )
            }),
    );
    let remove_index = 4 + move_targets.len();
    let drop_target = BookmarkDropTarget::Folder(uuid.clone());
    let folder_targeted = bookmark_drop_targeted(drag_state, &drop_target);
    let drag_item = BookmarkDragItem::Folder { uuid: uuid.clone() };
    let has_child_folders = folder_rows
        .iter()
        .any(|child| child.parent.as_deref() == Some(folder.uuid.as_str()));
    let folder_is_empty = !has_child_folders && folder.children.is_empty();

    rsx! {
        div {
            "data-bookmark-drop": "{uuid}",
            class: "flex flex-col gap-1",
            if editing() {
                div { class: "flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                    Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                        path { d: if collapsed { "m9 18 6-6-6-6" } else { "m6 9 6 6 6-6" } }
                    }
                    BookmarkNameInput {
                        draft,
                        class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none".to_string(),
                        placeholder: "Folder name".to_string(),
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
                LayoutContextMenu {
                    ContextMenuTrigger {
                        attributes: vec![],
                        div {
                            "data-bookmark-drag-source": "true",
                            class: if folder_targeted { "rounded-md ring-2 ring-ring" } else { "rounded-md" },
                            onpointerdown: {
                                let item = drag_item.clone();
                                move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                            },
                            SheetEntryRow {
                                active: false,
                                onclick: {
                                    let id = uuid.clone();
                                    move |event: MouseEvent| {
                                        if bookmark_drag_blocks_click(drag_state) {
                                            event.prevent_default();
                                            event.stop_propagation();
                                            return;
                                        }
                                        bookmark_cmd("toggle_folder", Some(id.clone()));
                                    }
                                },
                                Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                                    path { d: if collapsed { "m9 18 6-6-6-6" } else { "m6 9 6 6 6-6" } }
                                }
                                span { class: "min-w-0 flex-1 truncate text-ui font-medium text-foreground", "{folder.name}" }
                            }
                        }
                    }
                    SideSheetContextMenuContent {
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
                            "Bookmark Current Page"
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
                            "New Folder"
                        }
                        ContextMenuItem {
                            index: 3usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: {
                                let name = folder.name.clone();
                                move |_: String| begin_inline_rename(editing, draft, name.clone())
                            },
                            attributes: vec![],
                            "Rename Folder"
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
                            "Remove Folder"
                        }
                    }
                }
            }
            if creating_child() {
                div { class: "ml-3 flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                    Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                        path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                    }
                    BookmarkNameInput {
                        draft: child_draft,
                        class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none".to_string(),
                        placeholder: "Folder name".to_string(),
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
            if !collapsed {
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
                        div { class: "px-2 py-1.5 text-ui-xs text-muted-foreground", "Empty folder" }
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
    let mut folded = use_signal(|| false);

    rsx! {
        div { class: if pane.is_active && any_loading {
                "glass group mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg pane-loading-ring"
            } else if pane.is_active {
                "glass group mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg ring-2 ring-ring"
            } else {
                "glass group mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg"
            },
            div {
                class: "flex items-center transition-colors hover:bg-glass-hover",
                div { class: "flex min-w-0 flex-1 items-center gap-2 px-2.5 py-2",
                    div { class: "grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-foreground/[0.07] text-foreground ring-1 ring-inset ring-foreground/10",
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
                    aria_label: if folded() { "Expand stack" } else { "Collapse stack" },
                    title: if folded() { "Expand stack" } else { "Collapse stack" },
                    class: if folded() {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm bg-foreground/10 text-foreground"
                    } else {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10 hover:text-foreground"
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
            div { class: if folded() {
                    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                } else {
                    "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                },
                div { class: "overflow-hidden",
                    div { class: "flex flex-col gap-1 border-t border-foreground/10 p-1.5",
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
                    path: String::new(),
                });
            },
        }
    }
}

#[component]
fn SideSheetStackRow(stack: StackNode, pane_id: u64) -> Element {
    let folder_context: Signal<Vec<BookmarkFolderChoice>> = use_context();
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let folders = folder_context();
    let is_active = stack.is_active;
    let stack_index = stack.stack_index;
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
        LayoutContextMenu {
            ContextMenuTrigger {
                attributes: vec![],
                div {
                    "data-bookmark-drag-source": "true",
                    onpointerdown: {
                        let item = drag_item.clone();
                        move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                    },
                    id: "sidesheet-stack-{pane_id}-{stack_index}",
                    class: if is_active {
                        "glass flex h-9 cursor-default items-center gap-2 rounded-md px-2"
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
                        let _ = try_cef_bin_emit_rkyv(&crate::event::SideSheetCommandEvent {
                            command: "activate_stack".to_string(),
                            pane_id: pane_id.to_string(),
                            stack_index,
                            path: String::new(),
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
                        onpointerdown: move |evt| {
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
                                path: String::new(),
                            });
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
                    "Bookmark"
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
                        "Bookmark in {folder.label}"
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
                    "Pin"
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
