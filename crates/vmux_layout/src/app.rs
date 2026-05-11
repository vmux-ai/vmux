#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_layout::event::{
    HeaderCommandEvent, LAYOUT_STATE_EVENT, LayoutStateEvent, PANE_TREE_EVENT, PaneNode,
    PaneTreeEvent, RELOAD_EVENT, ReloadEvent, SPACES_EVENT, SpaceRow, SpacesCommandEvent,
    SpacesHostEvent, TABS_EVENT, TabNode, TabRow, TabsHostEvent,
};
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

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

fn favicon_src_for_tab(tab: &TabRow) -> Option<String> {
    if !tab.favicon_url.is_empty() {
        return Some(tab.favicon_url.clone());
    }
    host_for_favicon_fallback(&tab.url)
        .map(|h| format!("https://www.google.com/s2/favicons?domain={h}&sz=32"))
}

fn favicon_src(tab: &TabNode) -> Option<String> {
    if !tab.favicon_url.is_empty() {
        return Some(tab.favicon_url.clone());
    }
    host_for_favicon_fallback(&tab.url)
        .map(|h| format!("https://www.google.com/s2/favicons?domain={h}&sz=32"))
}

fn space_pill_class(is_active: bool) -> &'static str {
    if is_active {
        "group flex h-6 items-center gap-1 rounded-full bg-sidebar-primary pl-2.5 pr-1 text-ui-xs text-sidebar-primary-foreground shadow-sm"
    } else {
        "group flex h-6 items-center gap-1 rounded-full pl-2.5 pr-1 text-ui-xs text-muted-foreground hover:bg-glass-hover hover:text-foreground"
    }
}

fn space_close_button_class(is_active: bool) -> &'static str {
    if is_active {
        "flex h-4 w-4 cursor-pointer items-center justify-center rounded-full text-sidebar-primary-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-white/20"
    } else {
        "flex h-4 w-4 cursor-pointer items-center justify-center rounded-full text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-glass-hover hover:text-foreground"
    }
}

fn header_position_style(state: &LayoutStateEvent) -> String {
    let left = state.main_chrome_left();
    let top = vmux_layout::event::HEADER_TOP_PX;
    let height = state.header_height_total();
    format!("left:{left}px;top:{top}px;right:0;height:{height}px;")
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
    let side_sheet_style = format!(
        "left:0;top:0;bottom:0;width:{}px;padding-top:{}px;",
        state.side_sheet_width,
        vmux_layout::event::url_bar_top(),
    );

    rsx! {
        div { class: "fixed inset-0 pointer-events-none text-foreground",
            if state.side_sheet_open {
                aside {
                    class: "pointer-events-auto fixed min-h-0 overflow-hidden",
                    style: side_sheet_style,
                    div { class: "flex h-full min-h-0 flex-col",
                        SideSheetView {}
                    }
                }
            }
            if state.header_visible() {
                div {
                    class: "pointer-events-auto fixed",
                    style: header_position_style(&state),
                    HeaderView { titlebar_height: state.titlebar_height }
                }
            }
        }
    }
}

#[component]
fn HeaderView(titlebar_height: f32) -> Element {
    let mut tabs_state = use_signal(TabsHostEvent::default);
    let listener = use_bin_event_listener::<TabsHostEvent, _>(TABS_EVENT, move |data| {
        tabs_state.set(data);
    });

    let mut spaces_state = use_signal(SpacesHostEvent::default);
    let spaces_listener = use_bin_event_listener::<SpacesHostEvent, _>(SPACES_EVENT, move |data| {
        spaces_state.set(data);
    });

    let mut reload_key = use_signal(|| 0u32);
    let _reload_listener = use_bin_event_listener::<ReloadEvent, _>(RELOAD_EVENT, move |_| {
        reload_key.set(reload_key() + 1);
    });

    let TabsHostEvent {
        tabs,
        can_go_back,
        can_go_forward,
        is_zoomed: _,
    } = tabs_state();
    let SpacesHostEvent { spaces } = spaces_state();
    let active_row = tabs.iter().find(|t| t.is_active).cloned();
    let active_bg_color = active_row.as_ref().and_then(|r| r.bg_color.clone());
    let favicon_src = active_row.as_ref().and_then(favicon_src_for_tab);
    let mut favicon_error = use_signal(|| false);
    let mut prev_src = use_signal(|| None::<String>);
    if *prev_src.read() != favicon_src {
        prev_src.set(favicon_src.clone());
        favicon_error.set(false);
    }
    let listener_loading = (listener.is_loading)();
    let listener_error = (listener.error)();
    let spaces_loading = (spaces_listener.is_loading)();
    let spaces_error = (spaces_listener.error)();

    rsx! {
        div { class: "flex min-h-0 min-w-0 flex-1 flex-col text-foreground",
            div { class: "flex min-w-0 shrink-0 items-center gap-1 px-2 pb-1",
                if spaces_loading {
                    span { class: "text-ui text-muted-foreground", "Connecting..." }
                } else if let Some(err) = spaces_error {
                    span { class: "text-ui text-destructive", "{err}" }
                } else {
                    div { class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto",
                        for (idx, space) in spaces.iter().enumerate() {
                            SpacePill {
                                key: "{space.id}",
                                index: idx + 1,
                                space: space.clone(),
                                active_bg_color: active_bg_color.clone(),
                            }
                        }
                    }
                }
            }
            div { class: "flex min-w-0 shrink-0 items-center gap-1 px-2 pb-1",
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
                        favicon_src,
                        favicon_error,
                        bg_color: active_bg_color.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn HeaderAddressBar(
    active_row: Option<TabRow>,
    favicon_src: Option<String>,
    favicon_error: Signal<bool>,
    bg_color: Option<String>,
) -> Element {
    let has_content = active_row.as_ref().is_some_and(|t| !t.url.is_empty());
    let address_value = active_row
        .as_ref()
        .map(TabRow::address_text)
        .unwrap_or_default()
        .to_string();
    let placeholder = if has_content { "" } else { "New Stack" };

    let (bar_style, bar_class, input_class) = if let Some(ref color) = bg_color {
        let text_class = text_color_class_for_bg(color);
        (
            format!("background-color: {};", color),
            format!(
                "flex h-8 min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-lg px-2.5 shadow-sm {text_class}"
            ),
            format!(
                "min-w-0 flex-1 cursor-pointer bg-transparent text-ui outline-none placeholder:opacity-50 {text_class}"
            ),
        )
    } else if has_content {
        (
            String::new(),
            "flex h-8 min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-lg border border-glass-border bg-glass px-2.5 shadow-sm backdrop-blur-xl backdrop-saturate-150".to_string(),
            "min-w-0 flex-1 cursor-pointer bg-transparent text-ui text-foreground outline-none placeholder:text-muted-foreground".to_string(),
        )
    } else {
        (
            String::new(),
            "flex h-8 min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-lg border border-glass-border bg-glass px-2.5 backdrop-blur-md".to_string(),
            "min-w-0 flex-1 cursor-pointer bg-transparent text-ui text-foreground outline-none placeholder:text-muted-foreground".to_string(),
        )
    };

    rsx! {
        div {
            class: "{bar_class}",
            style: "{bar_style}",
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&HeaderCommandEvent {
                    header_command: "focus_address_bar".to_string(),
                });
            },
            if let Some(tab) = active_row.as_ref() {
                if tab.url.is_empty() {
                    Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                        path { d: "M5 12h14" }
                        path { d: "M12 5v14" }
                    }
                } else {
                    TabIcon { url: tab.url.clone(), title: tab.title.clone(), favicon_src, favicon_error }
                }
            }
            input {
                r#type: "text",
                readonly: true,
                class: "{input_class}",
                value: "{address_value}",
                placeholder: "{placeholder}",
            }
        }
    }
}

#[component]
fn TabIcon(
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
fn SpacePill(index: usize, space: SpaceRow, active_bg_color: Option<String>) -> Element {
    let id_switch = space.id.clone();
    let id_close = space.id.clone();
    let name = space.name.clone();
    let is_active = space.is_active;

    let (pill_style, pill_class, index_class, close_class) = if is_active {
        if let Some(ref color) = active_bg_color {
            let text_class = text_color_class_for_bg(color);
            (
                format!("background-color: {};", color),
                format!(
                    "group flex h-6 items-center gap-1 rounded-full pl-2.5 pr-1 text-ui-xs shadow-sm {text_class}"
                ),
                format!("font-mono {text_class}"),
                format!(
                    "flex h-4 w-4 cursor-pointer items-center justify-center rounded-full opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-white/20 {text_class}"
                ),
            )
        } else {
            (
                String::new(),
                space_pill_class(true).to_string(),
                "font-mono text-sidebar-primary-foreground".to_string(),
                space_close_button_class(true).to_string(),
            )
        }
    } else {
        (
            String::new(),
            space_pill_class(false).to_string(),
            "font-mono text-muted-foreground".to_string(),
            space_close_button_class(false).to_string(),
        )
    };

    rsx! {
        div {
            class: "{pill_class}",
            style: "{pill_style}",
            button {
                r#type: "button",
                title: "{name}",
                class: "flex min-w-0 cursor-pointer items-center gap-2",
                onclick: move |_| {
                    let _ = try_cef_bin_emit_rkyv(&SpacesCommandEvent {
                        command: "switch".to_string(),
                        space_id: Some(id_switch.clone()),
                    });
                },
                span { class: "{index_class}", "{index}" }
                span { class: "min-w-0 truncate", "{name}" }
            }
            button {
                r#type: "button",
                aria_label: "Close space",
                title: "Close space",
                class: "{close_class}",
                onclick: move |evt| {
                    evt.stop_propagation();
                    let _ = try_cef_bin_emit_rkyv(&SpacesCommandEvent {
                        command: "close".to_string(),
                        space_id: Some(id_close.clone()),
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
fn NewSpaceButton() -> Element {
    rsx! {
        button {
            r#type: "button",
            aria_label: "New space",
            title: "New space",
            class: "flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground active:bg-glass-active active:text-foreground",
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&SpacesCommandEvent {
                    command: "new".to_string(),
                    space_id: None,
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

    let PaneTreeEvent { panes } = tree_state();

    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-3 pt-2 text-foreground",
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
fn PaneSection(pane: PaneNode, index: usize) -> Element {
    let label = format!("Stack {}", index + 1);
    let pane_id = pane.id;
    let any_loading = pane.tabs.iter().any(|t| t.is_loading);

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
            div { class: "flex flex-col gap-px",
                for tab in pane.tabs.iter() {
                    SideSheetTabRow { tab: tab.clone(), pane_id }
                }
            }
        }
    }
}

#[component]
fn SideSheetTabRow(tab: TabNode, pane_id: u64) -> Element {
    let icon = favicon_src(&tab);
    let is_active = tab.is_active;
    let tab_index = tab.tab_index;
    let mut icon_error = use_signal(|| false);
    let mut prev_src = use_signal(|| None::<String>);
    if *prev_src.read() != icon {
        prev_src.set(icon.clone());
        icon_error.set(false);
    }

    rsx! {
        div {
            class: if is_active {
                "glass group flex cursor-default items-center gap-2 rounded-md px-2 py-1.5"
            } else {
                "group flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground hover:bg-glass-hover hover:text-foreground"
            },
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&vmux_layout::event::SideSheetCommandEvent {
                    command: "activate_tab".to_string(),
                    pane_id: pane_id.to_string(),
                    tab_index,
                });
            },
            TabIcon { url: tab.url.clone(), title: tab.title.clone(), favicon_src: icon, favicon_error: icon_error }
            span {
                class: if is_active {
                    "min-w-0 flex-1 truncate text-ui font-medium text-foreground"
                } else {
                    "min-w-0 flex-1 truncate text-ui"
                },
                "{tab.title}"
            }
            button {
                class: "cursor-pointer ml-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-sm opacity-0 transition-colors group-hover:opacity-100 hover:bg-foreground/10 active:bg-transparent",
                onclick: move |evt| {
                    evt.stop_propagation();
                    let _ = try_cef_bin_emit_rkyv(&vmux_layout::event::SideSheetCommandEvent {
                        command: "close_tab".to_string(),
                        pane_id: pane_id.to_string(),
                        tab_index,
                    });
                },
                span { class: "text-base leading-none", "x" }
            }
        }
    }
}
