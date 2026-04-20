#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_header::event::{HeaderCommandEvent, RELOAD_EVENT, TABS_EVENT, TabRow, TabsHostEvent};
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener};

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
    host_for_favicon_fallback(&tab.url).map(|h| {
        format!("https://www.google.com/s2/favicons?domain={h}&sz=32")
    })
}

#[component]
pub fn App() -> Element {
    let mut tabs_state = use_signal(TabsHostEvent::default);
    let listener = use_event_listener::<TabsHostEvent, _>(TABS_EVENT, move |data| {
        tabs_state.set(data);
    });

    let mut reload_key = use_signal(|| 0u32);
    let _reload_listener = use_event_listener::<(), _>(RELOAD_EVENT, move |_| {
        reload_key.set(reload_key() + 1);
    });

    let TabsHostEvent { tabs } = tabs_state();
    let active_row = tabs.iter().find(|t| t.is_active).cloned();
    let favicon_src = active_row.as_ref().and_then(favicon_src_for_tab);

    rsx! {
        div { class: "box-border flex min-h-0 min-w-0 flex-1 items-center gap-2 px-2 text-foreground",
            if (listener.is_loading)() {
                div { class: "col-span-3 flex w-full items-center px-3 py-2",
                    span { class: "text-ui text-muted-foreground", "Connecting…" }
                }
            } else if let Some(err) = (listener.error)() {
                div { class: "col-span-3 flex w-full items-center px-3 py-2",
                    span { class: "text-ui text-destructive", "{err}" }
                }
            } else {
                div { class: "flex min-w-0 items-center gap-1 justify-self-start",
                    NavButton { label: "Back", command: "prev_page",
                        Icon { class: "h-4 w-4",
                            path { d: "M19 12H5" }
                            path { d: "M12 19l-7-7 7-7" }
                        }
                    }
                    NavButton { label: "Forward", command: "next_page",
                        Icon { class: "h-4 w-4",
                            path { d: "M5 12h14" }
                            path { d: "M12 5l7 7-7 7" }
                        }
                    }
                    NavButton { label: "Reload", command: "reload",
                        span {
                            key: "{reload_key}",
                            class: if reload_key() > 0 { "inline-flex animate-spin-once" } else { "inline-flex" },
                            Icon { class: "h-4 w-4",
                                path { d: "M21 12a9 9 0 11-3-6.7L21 8" }
                                path { d: "M21 3v5h-5" }
                            }
                        }
                    }
                }
                div { class: "flex min-w-0 flex-1 items-center",
                    if let Some(tab) = active_row.as_ref() {
                        div {
                            class: "flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 rounded-full border border-glass-border bg-glass px-2.5 py-1 shadow-sm",
                            onclick: move |_| {
                                let _ = try_cef_emit_serde(&HeaderCommandEvent {
                                    header_command: "focus_address_bar".to_string(),
                                });
                            },
                            if let Some(src) = favicon_src.as_ref() {
                                img {
                                    class: "h-3.5 w-3.5 shrink-0 rounded-sm object-contain",
                                    src: "{src}",
                                }
                            } else {
                                div { class: "box-border h-3.5 w-3.5 shrink-0 rounded-sm border border-border bg-muted" }
                            }
                            span { class: "min-w-0 truncate text-ui text-foreground", "{tab.url}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NavButton(label: &'static str, command: &'static str, children: Element) -> Element {
    rsx! {
        button {
            r#type: "button",
            aria_label: label,
            title: label,
            class: "flex h-7 w-7 items-center justify-center rounded-md border border-glass-border bg-glass text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground active:bg-glass-active active:text-foreground",
            onclick: move |_| {
                let _ = try_cef_emit_serde(&HeaderCommandEvent {
                    header_command: command.to_string(),
                });
            },
            {children}
        }
    }
}
