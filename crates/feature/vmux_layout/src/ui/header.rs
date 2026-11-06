#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::bookmark::{
    BookmarkAddRequest, BookmarkPinUrlRequest, BookmarkToggleRequest, BookmarkUnpinRequest,
};
use vmux_ecs::event::team::{ProfileRow, TeamMemberRow, TeamOpenRequest, TeamProfileSwitchRequest};
use vmux_ui::cn::cn;
use vmux_ui::components::avatar::Avatar;
use vmux_ui::components::context_menu::{ContextMenuContent, ContextMenuItem, ContextMenuTrigger};
use vmux_ui::components::dropdown::{DropdownAlign, DropdownOption, DropdownPopup};
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

use super::LayoutDropdown;
use super::bookmark::LayoutContextMenu;
use super::extension::ExtensionBar;
use super::remote::RemoteControl;
use super::stack::{StackIcon, StackTitle};
use super::state::LayoutUi;
use super::tab_drag::TabDrag;
use super::window_drag::WindowDragRegion;
use crate::event::{
    HeaderAddressFocusRequest, HeaderBackRequest, HeaderForwardRequest, HeaderReloadRequest,
    StackNavigationState, StackRow, TabCloseRequest, TabCreateRequest, TabStripRow, TabStripState,
};

#[component]
pub(crate) fn HeaderView() -> Element {
    let layout = LayoutUi::current();
    let geometry = layout.use_selector(|ui| ui.layout);
    let Some(state) = geometry() else {
        return rsx! {};
    };
    if !state.header_visible() {
        return rsx! {};
    }
    let header_vars = format!(
        "--vmux-header-top:{}px;--vmux-header-left:{}px;--vmux-header-right:{}px;--vmux-header-height:{}px;--vmux-tab-row-pad-left:{}px;",
        state.header_top(),
        state.header_left(),
        state.header_right(),
        state.header_height,
        state.tab_row_pad_left(),
    );
    rsx! {
        div {
            class: "pointer-events-auto fixed top-[var(--vmux-header-top)] left-[var(--vmux-header-left)] right-[var(--vmux-header-right)] h-[var(--vmux-header-height)] overflow-hidden",
            style: "{header_vars}",
            div { class: "h-[76px]", HeaderContent {} }
        }
    }
}

#[component]
fn HeaderContent() -> Element {
    let layout = LayoutUi::current();
    let header = layout.use_selector(|ui| {
        (
            ui.stacks.clone(),
            ui.tab_strip.clone(),
            ui.header.clone(),
            ui.extensions.clone(),
            ui.remote.clone(),
            ui.reload_revision,
        )
    });
    let (stacks_state, tab_strip, header, extensions, remote, reload_key) = header();
    let stacks_state = stacks_state.unwrap_or_default();
    let projected_active = stacks_state
        .stacks
        .iter()
        .find(|row| row.is_active)
        .cloned();
    let tab_strip = tab_strip.unwrap_or_default();
    let crate::event::HeaderState {
        address: page_address,
        active: retained_active,
        metadata: retained_metadata,
        bookmarked: is_bookmarked,
        pinned_uuid,
        user,
        agents: _,
        profiles,
    } = header;
    let active_row = projected_active.or(retained_active);
    let active_metadata = active_row
        .as_ref()
        .map(|row| vmux_ecs::PageMetadata {
            title: row.title.clone(),
            url: row.url.clone(),
            icon: row.icon.clone(),
            bg_color: row.bg_color.clone(),
        })
        .or(retained_metadata);
    let extensions = extensions.extensions;
    let stacks_error = layout.error();
    let tabs_error = stacks_error.clone();
    let tab_order = layout.use_selector(|ui| ui.tab_strip.clone().unwrap_or_default().order);
    let tab_active = layout.use_selector(|ui| {
        ui.tab_strip.as_ref().and_then(|strip| {
            strip
                .tabs
                .iter()
                .find(|row| row.tab.is_active)
                .map(|row| row.tab.id.clone())
        })
    });
    let tab_drag = TabDrag::use_state(tab_order, tab_active);
    let StackNavigationState {
        stacks: _,
        can_go_back,
        can_go_forward,
        is_zoomed: _,
    } = stacks_state;
    let TabStripState {
        tabs,
        order: _,
        drag_region_revision: tab_drag_region_revision,
    } = tab_strip;
    let tab_metrics_style = TabDrag::metrics_style();
    let active_bg_color = active_row.as_ref().and_then(|r| r.bg_color.clone());
    let active_url = active_row
        .as_ref()
        .map(|r| r.url.clone())
        .unwrap_or_default();
    let show_bookmark = !active_url.is_empty();
    let is_pinned = pinned_uuid.is_some();

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
                    class: "h-9 shrink-0 self-stretch",
                    style: "margin-left:min(56px,var(--vmux-tab-row-pad-left));width:max(0px,calc(var(--vmux-tab-row-pad-left) - 56px));",
                }
                if let Some(err) = tabs_error {
                    span { class: "text-ui text-destructive", "{err}" }
                } else {
                    div {
                        class: "flex min-w-0 flex-1 items-center gap-[var(--tab-gap)] overflow-x-auto overflow-y-hidden pl-2",
                        style: "{tab_metrics_style}",
                        for (tab_index, tab) in tabs.iter().enumerate() {
                            Tab {
                                key: "{tab.tab.id}",
                                row: tab.clone(),
                                index: tab_index,
                                drag: tab_drag,
                                }
                        }
                        NewTabButton {}
                        WindowDragRegion {
                            id: "trailing",
                            revision: tab_drag_region_revision.clone(),
                            class: "h-9 min-w-0 flex-1 self-stretch",
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
                    NavButton { label: translate("layout-back"), disabled: !can_go_back, onclick: move |_| { let _ = send(&HeaderBackRequest); },
                        Icon { class: "h-4 w-4",
                            path { d: "M19 12H5" }
                            path { d: "M12 19l-7-7 7-7" }
                        }
                    }
                    NavButton { label: translate("layout-forward"), disabled: !can_go_forward, onclick: move |_| { let _ = send(&HeaderForwardRequest); },
                        Icon { class: "h-4 w-4",
                            path { d: "M5 12h14" }
                            path { d: "M12 5l7 7-7 7" }
                        }
                    }
                    NavButton { label: translate("layout-reload"), disabled: active_row.as_ref().is_none_or(|t| t.url.is_empty()), onclick: move |_| { let _ = send(&HeaderReloadRequest); },
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
                        page_address,
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
                                    let _ = send(&BookmarkUnpinRequest { uuid });
                                } else if let Some(metadata) = active_metadata.clone() {
                                    let _ = send(&BookmarkPinUrlRequest { metadata });
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
                    TeamFacepile { user, profiles }
                    ExtensionBar { extensions }
                    div { class: "hidden", RemoteControl { remote } }
                }
            }
        }
    }
}

fn url_row_cef(_bg_color: Option<&str>) -> (String, String) {
    (
        String::new(),
        "flex min-w-0 flex-1 shrink-0 items-center gap-1 rounded-t-[var(--radius)] bg-card px-2 text-foreground".to_string(),
    )
}

#[component]
fn Tab(row: TabStripRow, index: usize, drag: TabDrag) -> Element {
    let TabStripRow {
        tab,
        display_title,
        mut metadata,
    } = row;
    let visual = drag.visual(&tab.id, index);
    let id_switch = tab.id.clone();
    let id_close = tab.id.clone();
    let display_title = if display_title.is_empty() {
        translate("layout-tab")
    } else {
        display_title
    };
    let tooltip = display_title.clone();
    let is_active = drag.is_selected(&tab.id, tab.is_active);
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
        "group relative flex h-9 w-[var(--tab-width)] min-w-[var(--tab-width)] max-w-[var(--tab-width)] basis-[var(--tab-width)] shrink-0 grow-0 select-none items-start",
        cursor_classes,
    ]);
    let inactive_hover_classes = if visual.active() {
        ""
    } else {
        "bg-card/75 hover:bg-card hover:text-foreground"
    };

    let trunc = StackTitle::truncate_class(&display_title);
    let (surface_style, surface_class, title_class, close_class) = if is_active {
        (
            "--tab-bg:var(--card);background-color:var(--card);border-bottom-width:0;"
                .to_string(),
            cn([
                skirt_classes,
                "mt-1 flex h-8 w-full items-center gap-2 rounded-t-md border-b-0 px-3",
            ]),
            cn([
                "min-w-0 flex-1",
                trunc,
                "text-ui font-medium text-foreground",
            ]),
            "flex h-4 w-4 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10".to_string(),
        )
    } else {
        (
            String::new(),
            cn([
                "my-1 flex h-7 w-full items-center gap-2 rounded-lg px-3 text-muted-foreground",
                inactive_hover_classes,
            ]),
            cn([
                "min-w-0 flex-1",
                trunc,
                "text-ui",
            ]),
            "flex h-4 w-4 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10".to_string(),
        )
    };
    let tab_style = visual.style();

    if metadata.title.is_empty() {
        metadata.title = display_title.clone();
    }
    let bookmark_metadata = metadata;
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
                    drag.press(
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
                        let _ = send(&TabCloseRequest {
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
                    on_select: move |_: String| { let _ = send(&BookmarkAddRequest { metadata: bookmark_metadata.clone(), folder: None }); },
                    attributes: vec![],
                    {translate("layout-bookmark")}
                }
                ContextMenuItem {
                    index: 1usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| { let _ = send(&BookmarkPinUrlRequest { metadata: pin_metadata.clone() }); },
                    attributes: vec![],
                    {translate("layout-pin")}
                }
            }
        }
    }
}

#[component]
fn NewTabButton() -> Element {
    let mut pressed = use_signal(|| false);
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
                class: "absolute inset-0 flex appearance-none cursor-pointer items-center justify-center rounded-md border-0 bg-foreground/[0.04] text-muted-foreground shadow-none outline-none transition-colors hover:bg-foreground/[0.09] hover:text-foreground active:bg-foreground/[0.12] focus-visible:bg-foreground/[0.09] focus-visible:text-foreground",
                onpointerdown: move |event: Event<PointerData>| {
                    if event.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                        pressed.set(true);
                        let _ = send(&TabCreateRequest);
                    }
                },
                onpointercancel: move |_| pressed.set(false),
                onclick: move |_| {
                    if pressed() { pressed.set(false); return; }
                    let _ = send(&TabCreateRequest);
                },
                Icon { class: "h-4 w-4",
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
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
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
            onclick: move |event| {
                if !disabled {
                    onclick.call(event);
                }
            },
            {children}
        }
    }
}

#[component]
fn HeaderAddressBar(
    active_row: Option<StackRow>,
    bg_color: Option<String>,
    page_address: Option<vmux_api::address::PageAddress>,
) -> Element {
    let url = active_row
        .as_ref()
        .map(|row| row.url.clone())
        .or_else(|| page_address.as_ref().map(|address| address.url.clone()))
        .unwrap_or_default();
    let fallback = rsx! { HeaderAddressReadout { active_row, bg_color, page_address } };
    rsx! { vmux_ui::components::page_address::AddressSurface {
        url,
        onactivate: move |_| { let _ = send(&HeaderAddressFocusRequest); },
        {fallback}
    } }
}

#[component]
fn HeaderAddressReadout(
    active_row: Option<StackRow>,
    bg_color: Option<String>,
    page_address: Option<vmux_api::address::PageAddress>,
) -> Element {
    let dismiss_revision =
        LayoutUi::current().use_selector(|state| state.overlay_dismiss_revision)();
    if let Some(address) = page_address {
        return rsx! {
            div { class: "flex h-8 min-w-0 flex-1 items-center",
                vmux_ui::components::address::AddressTrail { state: address, dismiss_revision, onpopup: move |active| { let _ = send(&crate::event::LayoutOverlayEvent { id: "page-address".to_string(), active, kind: crate::event::LayoutOverlayKind::Popover }); } }
            }
        };
    }
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
            if !has_content {
                span { class: "truncate text-ui {empty_class}", {translate("layout-new-stack")} }
            } else {
                if !address.origin.is_empty() {
                    span {
                        class: "shrink-0 rounded-full bg-foreground/10 px-2 py-0.5 text-ui leading-tight",
                        "{address.origin}"
                    }
                }
                span { class: "truncate-fade min-w-0 flex-1 text-ui", title: "{address.rest}", "{address.rest}" }
            }
        }
    }
}

#[component]
fn TeamFacepile(user: Option<TeamMemberRow>, profiles: Vec<ProfileRow>) -> Element {
    let mut open = use_signal(|| false);
    if user.is_none() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "flex shrink-0 items-center pl-3 pr-3",
            if let Some(user) = user {
                LayoutDropdown {
                    id: "profiles",
                    keyboard_navigation: true,
                    open: open(),
                    on_dismiss: move |_| open.set(false),
                    class: "shrink-0",
                button {
                    r#type: "button",
                    "data-vmux-dropdown-trigger": "true",
                    class: "flex items-center gap-1.5 rounded-full bg-foreground/10 py-0.5 pl-0.5 pr-2.5 cursor-pointer transition-opacity hover:opacity-80",
                    title: translate("team-profiles"),
                    onclick: move |_| {
                        open.set(!open());
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
                if open() {
                    DropdownPopup { align: DropdownAlign::End, class: "w-64 p-1.5",
                        for profile in profiles {
                            DropdownOption {
                                key: "{profile.id}",
                                label: profile.name.clone(),
                                selected: profile.is_active,
                                onclick: move |_| {
                                    open.set(false);
                                    if !profile.is_active { let _ = send(&TeamProfileSwitchRequest { profile_id: profile.id.clone() }); }
                                },
                                leading: rsx! { Avatar { src: None, seed: profile.name.clone(), background: profile.color, alt: profile.name, class: "size-5 text-[8px]" } },
                            }
                        }
                        DropdownOption { label: translate("team-profiles"), selected: false,
                            onclick: move |_| { open.set(false); let _ = send(&TeamOpenRequest); },
                        }
                    }
                }
                }
            }
        }
    }
}
