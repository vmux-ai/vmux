#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_side_sheet::event::{PANE_TREE_EVENT, PaneNode, PaneTreeEvent, TabNode};
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

fn favicon_src(tab: &TabNode) -> Option<String> {
    if !tab.favicon_url.is_empty() {
        return Some(tab.favicon_url.clone());
    }
    host_for_favicon_fallback(&tab.url)
        .map(|h| format!("https://www.google.com/s2/favicons?domain={h}&sz=32"))
}

#[component]
pub fn App() -> Element {
    let mut tree_state = use_signal(PaneTreeEvent::default);
    let listener = use_event_listener::<PaneTreeEvent, _>(PANE_TREE_EVENT, move |data| {
        tree_state.set(data);
    });

    let PaneTreeEvent { panes } = tree_state();

    rsx! {
        div { class: "flex h-full flex-col overflow-y-auto bg-card px-2 py-3 text-foreground",
            if (listener.is_loading)() {
                div { class: "flex items-center px-2 py-1",
                    span { class: "text-ui text-muted-foreground", "Connecting…" }
                }
            } else if let Some(err) = (listener.error)() {
                div { class: "flex items-center px-2 py-1",
                    span { class: "text-ui text-destructive", "{err}" }
                }
            } else if panes.is_empty() {
                div { class: "flex items-center px-2 py-1",
                    span { class: "text-ui text-muted-foreground", "No panes" }
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
    let label = format!("Pane {}", index + 1);

    rsx! {
        div { class: "mb-1 flex flex-col",
            div {
                class: if pane.is_active {
                    "mb-0.5 rounded-md px-2 py-1 text-ui font-semibold text-foreground"
                } else {
                    "mb-0.5 rounded-md px-2 py-1 text-ui font-medium text-muted-foreground"
                },
                "{label}"
            }
            div { class: "flex flex-col gap-px pl-1",
                for tab in pane.tabs.iter() {
                    TabRow { tab: tab.clone(), is_active_pane: pane.is_active }
                }
            }
        }
    }
}

#[component]
fn TabRow(tab: TabNode, is_active_pane: bool) -> Element {
    let icon = favicon_src(&tab);

    rsx! {
        div {
            class: if is_active_pane {
                "flex items-center gap-2 rounded-md bg-muted px-2 py-1.5"
            } else {
                "flex items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
            },
            if let Some(src) = icon.as_ref() {
                img {
                    class: "h-4 w-4 shrink-0 rounded-sm object-contain",
                    src: "{src}",
                }
            } else {
                div { class: "box-border h-4 w-4 shrink-0 rounded-sm border border-border bg-muted" }
            }
            span {
                class: if is_active_pane {
                    "min-w-0 truncate text-ui font-medium text-foreground"
                } else {
                    "min-w-0 truncate text-ui"
                },
                "{tab.title}"
            }
        }
    }
}
