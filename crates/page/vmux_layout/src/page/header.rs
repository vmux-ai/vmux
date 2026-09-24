#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::bookmark::{BookmarkStateEvent, BookmarkToggleRequest};
use vmux_core::event::ExtRow;
use vmux_core::event::team::{TeamMemberRow, TeamRequest};
use vmux_core::{PageIcon, PageMetadata};
use vmux_ui::components::avatar::Avatar;
use vmux_ui::components::context_menu::{ContextMenuContent, ContextMenuItem, ContextMenuTrigger};
use vmux_ui::components::icon::Icon;
use vmux_ui::favicon::favicon_src_for_url;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::platform::sleep_ms;
use vmux_ui::util::cn;

use super::bookmark::{BookmarkIdCommand, BookmarkPageCommand, BookmarkTree, LayoutContextMenu};
use super::stack::{StackIcon, StackTitle};
use super::tab_drag::TabDrag;
use super::window_drag::WindowDragRegion;
use crate::event::{
    HeaderRequest, RemoteStateEvent, StackRow, StacksHostEvent, TabRow, TabsHostEvent, TabsRequest,
};
use crate::extension::ExtensionBar;
use crate::remote::RemoteControl;

#[component]
pub(crate) fn HeaderView(
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
        && (BookmarkTree::contains_url(&bookmarks.roots, &active_url)
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
                                    BookmarkIdCommand::Unpin.send(uuid);
                                } else if let Some(metadata) = active_metadata.clone() {
                                    BookmarkPageCommand::Pin.send(metadata, None);
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

    let trunc = StackTitle::truncate_class(&display_title);
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
                    on_select: move |_: String| BookmarkPageCommand::Add.send(bookmark_metadata.clone(), None),
                    attributes: vec![],
                    {translate("layout-bookmark")}
                }
                ContextMenuItem {
                    index: 1usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| BookmarkPageCommand::Pin.send(pin_metadata.clone(), None),
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
