#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_status_bar::event::{TABS_EVENT, TabRow, TabsHostEvent};
use vmux_ui::hooks::use_event_listener;

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

    let TabsHostEvent { tabs } = tabs_state();
    let active_row = tabs.iter().find(|t| t.is_active).cloned();
    let favicon_src = active_row.as_ref().and_then(favicon_src_for_tab);

    rsx! {
        div { class: "box-border flex min-h-0 min-w-0 flex-1 border-t border-border bg-card text-foreground",
            if (listener.is_loading)() {
                div { class: "flex w-full items-center px-3 py-2",
                    span { class: "text-ui text-muted-foreground", "Connecting…" }
                }
            } else if let Some(err) = (listener.error)() {
                div { class: "flex w-full items-center px-3 py-2",
                    span { class: "text-ui text-destructive", "{err}" }
                }
            } else {
                div { class: "flex min-h-0 min-w-0 flex-1 items-center",
                    div { class: "flex min-h-0 min-w-0 flex-1 flex-row items-stretch gap-1 overflow-x-auto px-3 py-2",
                        for row in tabs {
                            div {
                                class: if row.is_active {
                                    "flex max-w-40 min-w-0 shrink-0 items-center rounded-md border border-primary bg-muted px-2 py-1 shadow-sm"
                                } else {
                                    "flex max-w-40 min-w-0 shrink-0 items-center rounded-md border border-transparent px-2 py-1 text-muted-foreground hover:border-border hover:bg-muted hover:text-foreground"
                                },
                                span {
                                    class: if row.is_active {
                                        "truncate text-ui font-medium text-foreground"
                                    } else {
                                        "truncate text-ui"
                                    },
                                    "{row.title}"
                                }
                            }
                        }
                    }
                }
                div { class: "flex min-w-0 shrink-0 items-center justify-center px-3 py-2",
                    if let Some(tab) = active_row.as_ref() {
                        div { class: "flex min-w-0 max-w-md items-center gap-1.5 rounded-full border border-border bg-muted px-2.5 py-1 shadow-sm",
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
                div { class: "min-w-0 flex-1" }
            }
        }
    }
}
