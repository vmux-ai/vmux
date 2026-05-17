#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_layout::event::{
    HeaderCommandEvent, LAYOUT_STATE_EVENT, LayoutStateEvent, PANE_TREE_EVENT, PaneNode,
    PaneTreeEvent, RELOAD_EVENT, ReloadEvent, STACKS_EVENT, StackNode, StackRow, StacksHostEvent,
    TABS_EVENT, TabRow, TabsCommandEvent, TabsHostEvent,
};
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;

fn parse_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix('#')
        && rest.len() == 6
    {
        let r = u8::from_str_radix(&rest[0..2], 16).ok()?;
        let g = u8::from_str_radix(&rest[2..4], 16).ok()?;
        let b = u8::from_str_radix(&rest[4..6], 16).ok()?;
        return Some((r, g, b));
    }
    if let Some(inner) = trimmed
        .strip_prefix("rgb(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 3 {
            return Some((
                parts[0].parse().ok()?,
                parts[1].parse().ok()?,
                parts[2].parse().ok()?,
            ));
        }
    }
    None
}

fn text_color_class_for_bg(bg: &str) -> &'static str {
    parse_rgb(bg)
        .map(|(r, g, b)| {
            let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            if lum > 128.0 {
                "text-zinc-900"
            } else {
                "text-zinc-100"
            }
        })
        .unwrap_or("text-foreground")
}

fn host_for_favicon_fallback(page_url: &str) -> Option<&str> {
    let s = page_url.trim();
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))?;
    rest.split(&['/', '?', '#'][..])
        .next()
        .filter(|h| !h.is_empty())
}

fn agent_host(url: &str) -> Option<&'static str> {
    const AGENTS: &[(&str, &str)] = &[
        ("vibe", "chat.mistral.ai"),
        ("claude", "claude.ai"),
        ("codex", "chatgpt.com"),
    ];
    for &(kind, host) in AGENTS {
        if url.starts_with(&format!("vmux://agent/{kind}/cli/"))
            || url.starts_with(&format!("vmux://agent/{kind}/"))
        {
            return Some(host);
        }
    }
    None
}

fn favicon_src_for_url(favicon_url: &str, url: &str) -> Option<String> {
    if !favicon_url.is_empty() {
        return Some(favicon_url.to_string());
    }
    if let Some(host) = agent_host(url) {
        return Some(format!(
            "https://www.google.com/s2/favicons?domain={host}&sz=32"
        ));
    }
    host_for_favicon_fallback(url)
        .map(|h| format!("https://www.google.com/s2/favicons?domain={h}&sz=32"))
}

fn favicon_src_for_stack_node(stack: &StackNode) -> Option<String> {
    favicon_src_for_url(&stack.favicon_url, &stack.url)
}

fn favicon_src_for_tab(tab: &TabRow) -> Option<String> {
    favicon_src_for_url(&tab.favicon_url, &tab.url)
}

fn format_address(stack: &StackRow) -> String {
    if stack.url.starts_with("vmux://") {
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
pub fn App() -> Element {
    use_theme();

    let mut layout_state = use_signal(LayoutStateEvent::default);
    let _layout_listener =
        use_bin_event_listener::<LayoutStateEvent, _>(LAYOUT_STATE_EVENT, move |data| {
            layout_state.set(data);
        });

    let state = layout_state();
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
    let side_sheet_style = format!(
        "left:0;top:0;bottom:0;width:{}px;padding-top:{}px;",
        state.side_sheet_width,
        vmux_layout::event::url_bar_top(),
    );
    let header_style = format!(
        "left:{}px;top:0;right:{}px;height:{}px;",
        state.main_chrome_left(),
        vmux_layout::event::WINDOW_PAD_PX,
        state.header_height_total(),
    );

    rsx! {
        div { class: "fixed inset-0 pointer-events-none text-foreground",
            if state.side_sheet_open {
                aside {
                    class: "pointer-events-auto fixed min-h-0 overflow-hidden",
                    style: "{side_sheet_style}",
                    div { class: "flex h-full min-h-0 flex-col",
                        SideSheetView {}
                    }
                }
            }
            if state.header_visible() {
                div {
                    class: "pointer-events-auto fixed",
                    style: "{header_style}",
                    HeaderView { titlebar_height: state.titlebar_height }
                }
            }
        }
    }
}

#[component]
fn HeaderView(titlebar_height: f32) -> Element {
    let mut stacks_state = use_signal(StacksHostEvent::default);
    let listener = use_bin_event_listener::<StacksHostEvent, _>(STACKS_EVENT, move |data| {
        stacks_state.set(data);
    });

    let mut tabs_state = use_signal(TabsHostEvent::default);
    let tabs_listener = use_bin_event_listener::<TabsHostEvent, _>(TABS_EVENT, move |data| {
        tabs_state.set(data);
    });

    let mut reload_key = use_signal(|| 0u32);
    let _reload_listener = use_bin_event_listener::<ReloadEvent, _>(RELOAD_EVENT, move |_| {
        reload_key.set(reload_key() + 1);
    });

    let StacksHostEvent {
        stacks,
        can_go_back,
        can_go_forward,
        is_zoomed: _,
    } = stacks_state();
    let TabsHostEvent { tabs } = tabs_state();
    let active_row = stacks.iter().find(|t| t.is_active).cloned();
    let active_bg_color = active_row.as_ref().and_then(|r| r.bg_color.clone());
    let listener_loading = (listener.is_loading)();
    let listener_error = (listener.error)();
    let tabs_loading = (tabs_listener.is_loading)();
    let tabs_error = (tabs_listener.error)();

    let (url_row_style, url_row_class) = url_row_chrome(active_bg_color.as_deref());
    let outer_style = format!("padding-top:{titlebar_height}px;");

    rsx! {
        div {
            class: "flex h-full min-h-0 min-w-0 flex-col text-foreground",
            style: "{outer_style}",
            div { class: "flex min-w-0 shrink-0 items-center gap-1 px-2",
                if tabs_loading {
                    span { class: "text-ui text-muted-foreground", "Connecting..." }
                } else if let Some(err) = tabs_error {
                    span { class: "text-ui text-destructive", "{err}" }
                } else {
                    div { class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto pl-2",
                        for tab in tabs.iter() {
                            Tab { key: "{tab.id}", tab: tab.clone() }
                        }
                        NewTabButton {}
                    }
                }
            }
            div {
                class: "{url_row_class}",
                style: "{url_row_style}",
                if listener_loading {
                    span { class: "text-ui text-muted-foreground", "Connecting..." }
                } else if let Some(err) = listener_error {
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
                            class: if reload_key() > 0 { "inline-flex animate-spin-once" } else { "inline-flex" },
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
                }
            }
        }
    }
}

fn url_row_chrome(bg_color: Option<&str>) -> (String, String) {
    if let Some(color) = bg_color {
        let text_class = text_color_class_for_bg(color);
        (
            format!("background-color: {color};"),
            format!(
                "flex min-w-0 flex-1 shrink-0 items-center gap-1 rounded-t-lg px-2 {text_class}"
            ),
        )
    } else {
        (
            String::new(),
            "flex min-w-0 flex-1 shrink-0 items-center gap-1 rounded-t-lg px-2 bg-glass backdrop-blur-xl backdrop-saturate-150 text-foreground".to_string(),
        )
    }
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
fn StackIcon(
    url: String,
    title: String,
    favicon_src: Option<String>,
    favicon_error: Signal<bool>,
) -> Element {
    rsx! {
        if url.starts_with("vmux://terminal") {
            Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                path { d: "M4 17 10 11 4 5" }
                path { d: "M12 19h8" }
            }
        } else if let Some(src) = favicon_src.as_ref() {
            if favicon_error() {
                GlobeIcon {}
            } else {
                img {
                    class: "h-4 w-4 shrink-0 rounded-sm object-contain",
                    src: "{src}",
                    onerror: move |_| favicon_error.set(true),
                }
            }
        } else if title == "New Stack" && url.is_empty() {
            Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                path { d: "M5 12h14" }
                path { d: "M12 5v14" }
            }
        } else {
            GlobeIcon {}
        }
    }
}

#[component]
fn GlobeIcon() -> Element {
    rsx! {
        Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
            path { d: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z" }
            path { d: "M2 12h20" }
            path { d: "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z" }
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
    let icon_src = favicon_src_for_tab(&tab);
    let mut icon_error = use_signal(|| false);
    let mut prev_src = use_signal(|| None::<String>);
    if *prev_src.read() != icon_src {
        prev_src.set(icon_src.clone());
        icon_error.set(false);
    }

    let skirt_classes = "relative \
        before:content-[''] before:absolute before:bottom-0 before:-left-2 before:h-2 before:w-2 before:pointer-events-none \
        before:[background:radial-gradient(circle_at_top_left,transparent_0,transparent_8px,var(--tab-bg)_8px)] \
        after:content-[''] after:absolute after:bottom-0 after:-right-2 after:h-2 after:w-2 after:pointer-events-none \
        after:[background:radial-gradient(circle_at_top_right,transparent_0,transparent_8px,var(--tab-bg)_8px)]";

    let (tab_style, tab_class, title_class, close_class) = if is_active {
        if let Some(ref color) = tab.bg_color {
            let text_class = text_color_class_for_bg(color);
            (
                format!(
                    "background-color:{color};--tab-bg:{color};max-width:200px;margin-bottom:-3px;padding-bottom:3px;"
                ),
                format!(
                    "{skirt_classes} group flex h-7 min-w-0 items-center gap-1.5 rounded-t-md pl-2 pr-2 {text_class}"
                ),
                format!("min-w-0 truncate text-ui-xs font-medium {text_class}"),
                format!(
                    "flex h-4 w-4 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-white/20 {text_class}"
                ),
            )
        } else {
            (
                "max-width:200px;margin-bottom:-3px;padding-bottom:3px;--tab-bg:var(--glass);".to_string(),
                format!(
                    "{skirt_classes} glass group flex h-7 min-w-0 items-center gap-1.5 rounded-t-md border-b-0 pl-2 pr-2"
                ),
                "min-w-0 truncate text-ui-xs font-medium text-foreground".to_string(),
                "flex h-4 w-4 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10".to_string(),
            )
        }
    } else {
        (
            "max-width:200px;".to_string(),
            "group flex h-7 min-w-0 items-center gap-1.5 rounded-md pl-2 pr-2 text-muted-foreground hover:bg-glass-hover hover:text-foreground".to_string(),
            "min-w-0 truncate text-ui-xs".to_string(),
            "flex h-4 w-4 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10".to_string(),
        )
    };

    rsx! {
        div {
            class: "{tab_class}",
            style: "{tab_style}",
            button {
                r#type: "button",
                title: "{tooltip}",
                class: "flex min-w-0 flex-1 cursor-pointer items-center gap-1.5",
                onclick: move |_| {
                    let _ = try_cef_bin_emit_rkyv(&TabsCommandEvent {
                        command: "switch".to_string(),
                        tab_id: Some(id_switch.clone()),
                    });
                },
                StackIcon {
                    url: tab.url.clone(),
                    title: display_title.clone(),
                    favicon_src: icon_src,
                    favicon_error: icon_error,
                }
                span { class: "{title_class}", "{display_title}" }
            }
            button {
                r#type: "button",
                aria_label: "Close tab",
                title: "Close tab",
                class: "{close_class}",
                onclick: move |evt| {
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
fn SideSheetView() -> Element {
    let mut tree_state = use_signal(PaneTreeEvent::default);
    let listener = use_bin_event_listener::<PaneTreeEvent, _>(PANE_TREE_EVENT, move |data| {
        tree_state.set(data);
    });

    let mut spaces_state = use_signal(vmux_space::event::SpacesListEvent::default);
    let _spaces_listener = use_bin_event_listener::<vmux_space::event::SpacesListEvent, _>(
        vmux_space::event::SPACES_LIST_EVENT,
        move |data| {
            spaces_state.set(data);
        },
    );

    let PaneTreeEvent { panes } = tree_state();
    let spaces = spaces_state().spaces;

    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-3 pt-2 text-foreground",
            if !spaces.is_empty() {
                div { class: "mb-2 flex flex-col gap-px",
                    for space in spaces.iter() {
                        SideSheetSpaceRow { key: "{space.id}", space: space.clone() }
                    }
                }
            }
            if (listener.is_loading)() {
                div { class: "flex items-center px-2 py-1",
                    span { class: "text-ui text-muted-foreground", "Connecting..." }
                }
            } else if let Some(err) = (listener.error)() {
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

#[component]
fn SideSheetSpaceRow(space: vmux_space::event::SpaceRow) -> Element {
    let is_active = space.is_active;
    rsx! {
        button {
            r#type: "button",
            class: if is_active {
                "glass group flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-foreground"
            } else {
                "group flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground hover:bg-glass-hover hover:text-foreground"
            },
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&vmux_space::event::SpaceCommandEvent {
                    command: "open_page".to_string(),
                    space_id: None,
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
                class: if is_active {
                    "min-w-0 flex-1 truncate text-ui font-medium text-foreground text-left"
                } else {
                    "min-w-0 flex-1 truncate text-ui text-left"
                },
                "{space.name}"
            }
        }
    }
}

#[component]
fn PaneSection(pane: PaneNode, index: usize) -> Element {
    let label = format!("Pane {}", index + 1);
    let pane_id = pane.id;
    let any_loading = pane.stacks.iter().any(|s| s.is_loading);

    rsx! {
        div { class: if pane.is_active && any_loading {
                "glass mb-2 flex flex-col rounded-lg p-1.5 pane-loading-ring"
            } else if pane.is_active {
                "glass mb-2 flex flex-col rounded-lg p-1.5 ring-2 ring-ring"
            } else {
                "glass mb-2 flex flex-col rounded-lg p-1.5"
            },
            div {
                class: if pane.is_active {
                    "mb-0.5 rounded-md px-2 py-1 text-ui font-semibold text-foreground"
                } else {
                    "mb-0.5 rounded-md px-2 py-1 text-ui font-medium text-muted-foreground"
                },
                "{label}"
            }
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

#[component]
fn NewStackRow(pane_id: u64) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "group flex h-9 cursor-pointer items-center gap-2 rounded-md px-2 text-left text-muted-foreground hover:bg-glass-hover hover:text-foreground",
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&vmux_layout::event::SideSheetCommandEvent {
                    command: "new_stack".to_string(),
                    pane_id: pane_id.to_string(),
                    stack_index: 0,
                });
            },
            Icon { class: "h-4 w-4 shrink-0",
                path { d: "M12 5v14" }
                path { d: "M5 12h14" }
            }
            span { class: "min-w-0 flex-1 truncate text-ui font-medium", "New Stack" }
        }
    }
}

#[component]
fn SideSheetStackRow(stack: StackNode, pane_id: u64) -> Element {
    let icon = favicon_src_for_stack_node(&stack);
    let is_active = stack.is_active;
    let stack_index = stack.stack_index;
    let mut icon_error = use_signal(|| false);
    let mut prev_src = use_signal(|| None::<String>);
    if *prev_src.read() != icon {
        prev_src.set(icon.clone());
        icon_error.set(false);
    }

    rsx! {
        div {
            class: if is_active {
                "glass group flex h-9 cursor-default items-center gap-2 rounded-md px-2"
            } else {
                "group flex h-9 cursor-pointer items-center gap-2 rounded-md px-2 text-muted-foreground hover:bg-glass-hover hover:text-foreground"
            },
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&vmux_layout::event::SideSheetCommandEvent {
                    command: "activate_stack".to_string(),
                    pane_id: pane_id.to_string(),
                    stack_index,
                });
            },
            StackIcon { url: stack.url.clone(), title: stack.title.clone(), favicon_src: icon, favicon_error: icon_error }
            span {
                class: if is_active {
                    "min-w-0 flex-1 truncate text-ui font-medium text-foreground"
                } else {
                    "min-w-0 flex-1 truncate text-ui"
                },
                "{stack.title}"
            }
            button {
                r#type: "button",
                aria_label: "Close stack",
                title: "Close stack",
                class: "ml-auto flex h-6 w-6 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10",
                onclick: move |evt| {
                    evt.stop_propagation();
                    let _ = try_cef_bin_emit_rkyv(&vmux_layout::event::SideSheetCommandEvent {
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
