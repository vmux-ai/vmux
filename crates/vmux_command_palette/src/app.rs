#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command_palette::event::{
    PaletteActionEvent, PaletteCommandEntry, PaletteOpenEvent, PaletteTab, PALETTE_OPEN_EVENT,
};
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener};

#[derive(Clone, PartialEq)]
enum ResultItem {
    Tab {
        title: String,
        url: String,
        pane_id: u64,
        tab_index: usize,
    },
    Command {
        id: String,
        name: String,
        shortcut: String,
    },
    Navigate {
        url: String,
    },
}

fn filter_results(
    query: &str,
    tabs: &[PaletteTab],
    commands: &[PaletteCommandEntry],
) -> Vec<ResultItem> {
    let q = query.trim();
    if q.is_empty() {
        let mut items: Vec<ResultItem> = tabs
            .iter()
            .map(|t| ResultItem::Tab {
                title: t.title.clone(),
                url: t.url.clone(),
                pane_id: t.pane_id,
                tab_index: t.tab_index,
            })
            .collect();
        items.extend(commands.iter().map(|c| ResultItem::Command {
            id: c.id.clone(),
            name: c.name.clone(),
            shortcut: c.shortcut.clone(),
        }));
        return items;
    }

    let commands_only = q.starts_with('>');
    let search = if commands_only { q[1..].trim() } else { q };
    let search_lower = search.to_lowercase();

    let mut items = Vec::new();

    if !commands_only {
        for t in tabs {
            if t.title.to_lowercase().contains(&search_lower)
                || t.url.to_lowercase().contains(&search_lower)
            {
                items.push(ResultItem::Tab {
                    title: t.title.clone(),
                    url: t.url.clone(),
                    pane_id: t.pane_id,
                    tab_index: t.tab_index,
                });
            }
        }
    }

    for c in commands {
        if c.name.to_lowercase().contains(&search_lower) || c.id.contains(&search_lower) {
            items.push(ResultItem::Command {
                id: c.id.clone(),
                name: c.name.clone(),
                shortcut: c.shortcut.clone(),
            });
        }
    }

    if !commands_only && !search.is_empty() {
        items.push(ResultItem::Navigate {
            url: search.to_string(),
        });
    }

    items
}

fn emit_action(action: &str, value: &str) {
    let _ = try_cef_emit_serde(&PaletteActionEvent {
        action: action.to_string(),
        value: value.to_string(),
    });
}

#[component]
pub fn App() -> Element {
    let mut state = use_signal(PaletteOpenEvent::default);
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);

    let _listener = use_event_listener::<PaletteOpenEvent, _>(PALETTE_OPEN_EVENT, move |data| {
        query.set(data.url.clone());
        selected.set(0);
        state.set(data);
    });

    let PaletteOpenEvent {
        url: _,
        tabs,
        commands,
    } = state();
    let q = query();
    let results = filter_results(&q, &tabs, &commands);
    let sel = selected().min(results.len().saturating_sub(1));

    let execute = move |item: &ResultItem| match item {
        ResultItem::Tab {
            pane_id, tab_index, ..
        } => {
            emit_action("switch_tab", &format!("{pane_id}:{tab_index}"));
        }
        ResultItem::Command { id, .. } => {
            emit_action("command", id);
        }
        ResultItem::Navigate { url } => {
            emit_action("navigate", url);
        }
    };

    rsx! {
        div {
            class: "flex h-full w-full items-start justify-center bg-black/50 pt-[15%]",
            onclick: move |_| { emit_action("dismiss", ""); },
            div {
                class: "flex w-full max-w-lg flex-col rounded-xl border border-border bg-card shadow-2xl",
                onclick: move |e| { e.stop_propagation(); },
                div { class: "p-2",
                    input {
                        r#type: "text",
                        class: "w-full rounded-lg bg-muted px-3 py-2 text-sm text-foreground outline-none placeholder:text-muted-foreground",
                        placeholder: "Type a URL, search tabs, or > for commands...",
                        value: "{q}",
                        autofocus: true,
                        oninput: move |e| {
                            query.set(e.value());
                            selected.set(0);
                        },
                        onkeydown: move |e| {
                            match e.key() {
                                Key::Escape => { emit_action("dismiss", ""); }
                                Key::ArrowDown => {
                                    let max = results.len().saturating_sub(1);
                                    selected.set((sel + 1).min(max));
                                }
                                Key::ArrowUp => {
                                    selected.set(sel.saturating_sub(1));
                                }
                                Key::Enter => {
                                    if let Some(item) = results.get(sel) {
                                        execute(item);
                                    } else if !q.is_empty() {
                                        emit_action("navigate", &q);
                                    }
                                }
                                _ => {}
                            }
                        },
                    }
                }
                if !results.is_empty() {
                    div { class: "max-h-64 overflow-y-auto border-t border-border p-1",
                        for (i, item) in results.iter().enumerate() {
                            div {
                                key: "{i}",
                                class: if i == sel {
                                    "flex cursor-pointer items-center justify-between rounded-lg bg-muted px-3 py-1.5"
                                } else {
                                    "flex cursor-pointer items-center justify-between rounded-lg px-3 py-1.5 hover:bg-muted/50"
                                },
                                onclick: {
                                    let item = item.clone();
                                    move |_| { execute(&item); }
                                },
                                match item {
                                    ResultItem::Tab { title, url, .. } => rsx! {
                                        div { class: "flex min-w-0 flex-col",
                                            span { class: "truncate text-sm text-foreground", "{title}" }
                                            span { class: "truncate text-xs text-muted-foreground", "{url}" }
                                        }
                                        span { class: "ml-2 shrink-0 text-xs text-muted-foreground", "Tab" }
                                    },
                                    ResultItem::Command { name, shortcut, .. } => rsx! {
                                        span { class: "text-sm text-foreground", "{name}" }
                                        span { class: "ml-2 shrink-0 rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground", "{shortcut}" }
                                    },
                                    ResultItem::Navigate { url } => rsx! {
                                        span { class: "text-sm text-foreground", "Navigate to {url}" }
                                        span { class: "ml-2 shrink-0 text-xs text-muted-foreground", "\u{21b5}" }
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
