#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_side_sheet::event::{
    PANE_TREE_EVENT, PaneNode, PaneTreeEvent, SideSheetCommandEvent, TabNode,
};
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
        div { class: "flex h-full flex-col overflow-y-auto px-2 py-3 text-foreground",
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
    let pane_id = pane.id;

    rsx! {
        div { class: "glass mb-2 flex flex-col rounded-lg p-1.5",
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
                    TabRow { tab: tab.clone(), pane_id }
                }
            }
        }
    }
}

#[component]
fn TabRow(tab: TabNode, pane_id: u64) -> Element {
    let icon = favicon_src(&tab);
    let is_active = tab.is_active;
    let tab_index = tab.tab_index;

    rsx! {
        div {
            class: if is_active {
                "group flex cursor-default items-center gap-2 rounded-md bg-glass px-2 py-1.5 border border-glass-border"
            } else {
                "group flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground hover:bg-glass-hover hover:text-foreground"
            },
            onclick: move |_| {
                let _ = try_cef_emit_serde(&SideSheetCommandEvent {
                    command: "activate_tab".to_string(),
                    pane_id: pane_id.to_string(),
                    tab_index,
                });
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
                class: if is_active {
                    "min-w-0 flex-1 truncate text-ui font-medium text-foreground"
                } else {
                    "min-w-0 flex-1 truncate text-ui"
                },
                "{tab.title}"
            }
            button {
                class: "cursor-pointer ml-auto flex h-4 w-4 shrink-0 items-center justify-center rounded-sm opacity-0 transition-colors group-hover:opacity-100 hover:bg-foreground/10 active:bg-transparent",
                onclick: move |evt| {
                    evt.stop_propagation();
                    let _ = try_cef_emit_serde(&SideSheetCommandEvent {
                        command: "close_tab".to_string(),
                        pane_id: pane_id.to_string(),
                        tab_index,
                    });
                },
                span { class: "text-[10px] leading-none", "\u{00d7}" }
            }
        }
    }
}
