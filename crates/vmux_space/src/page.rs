#![allow(non_snake_case)]

use dioxus::prelude::*;
use crate::event::{SPACES_LIST_EVENT, SpaceCommandEvent, SpaceRow, SpacesListEvent};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(SpacesListEvent::default);
    let mut selected = use_signal(|| 0usize);

    let _listener = use_bin_event_listener::<SpacesListEvent, _>(SPACES_LIST_EVENT, move |data| {
        selected.set(0);
        state.set(data);
    });

    use_effect(move || {
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("spaces-root"))
            && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = html.focus();
        }
    });

    let spaces = state.read().spaces.clone();
    let count = spaces.len();
    let sel = selected().min(count.saturating_sub(1));
    let active_name = spaces
        .iter()
        .find(|space| space.is_active)
        .map(|space| space.name.clone())
        .unwrap_or_else(|| "Space 1".to_string());
    let selected_space_id = spaces.get(sel).map(|space| space.id.clone());
    let selected_space_deletable = count > 1 && spaces.get(sel).is_some();

    rsx! {
        div {
            id: "spaces-root",
            tabindex: "0",
            class: "flex h-full min-h-0 flex-col bg-background text-foreground outline-none",
            onkeydown: move |e| {
                let ctrl = e.modifiers().contains(Modifiers::CONTROL);
                let down = (!ctrl && (e.code() == Code::KeyJ || e.key() == Key::ArrowDown))
                    || (ctrl && e.code() == Code::KeyN);
                let up = (!ctrl && (e.code() == Code::KeyK || e.key() == Key::ArrowUp))
                    || (ctrl && e.code() == Code::KeyP);
                if down {
                    e.prevent_default();
                    let max = count.saturating_sub(1);
                    selected.set((sel + 1).min(max));
                } else if up {
                    e.prevent_default();
                    selected.set(sel.saturating_sub(1));
                } else if e.key() == Key::Enter {
                    e.prevent_default();
                    if let Some(id) = selected_space_id.clone() {
                        emit_command("attach", Some(id), None);
                    }
                } else if e.key() == Key::Delete || e.key() == Key::Backspace {
                    e.prevent_default();
                    if selected_space_deletable
                        && let Some(id) = selected_space_id.clone()
                    {
                        emit_command("delete", Some(id), None);
                    }
                }
            },
            div { class: "flex items-center justify-between border-b border-border px-5 py-4",
                div { class: "min-w-0",
                    h1 { class: "text-lg font-semibold", "Spaces" }
                    div { class: "mt-1 truncate text-xs text-muted-foreground", "{active_name}" }
                }
                button {
                    class: "rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground transition-colors hover:border-foreground/30 hover:bg-muted",
                    onclick: move |_| {
                        emit_command("new", None, Some(format!("Space {}", count + 1)));
                    },
                    "New"
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-3",
                if spaces.is_empty() {
                    div { class: "flex h-full items-center justify-center text-sm text-muted-foreground", "No spaces" }
                } else {
                    div { class: "flex flex-col gap-2",
                        for (index, space) in spaces.iter().enumerate() {
                            SpaceRowView {
                                key: "{space.id}",
                                space: space.clone(),
                                selected: index == sel,
                                deletable: count > 1,
                            }
                        }
                    }
                }
            }
        }
    }
}

fn emit_command(command: &str, space_id: Option<String>, name: Option<String>) {
    let _ = try_cef_bin_emit_rkyv(&SpaceCommandEvent {
        command: command.to_string(),
        space_id,
        name,
    });
}

#[component]
fn SpaceRowView(space: SpaceRow, selected: bool, deletable: bool) -> Element {
    let nav_id = space.id.clone();
    let delete_id = space.id.clone();
    let class = if selected {
        "flex cursor-pointer items-center justify-between rounded-lg border border-foreground/30 bg-muted px-3 py-3"
    } else {
        "flex cursor-pointer items-center justify-between rounded-lg border border-border bg-card px-3 py-3 transition-colors hover:border-foreground/30 hover:bg-muted/60"
    };
    let tab_label = if space.tab_count == 1 {
        "1 tab".to_string()
    } else {
        format!("{} tabs", space.tab_count)
    };

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| {
                emit_command("attach", Some(nav_id.clone()), None);
            },
            div { class: "min-w-0",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "truncate text-sm font-medium text-foreground", "{space.name}" }
                    if space.is_active {
                        span { class: "rounded-full bg-blue-500/15 px-2 py-0.5 text-xs text-blue-300", "active" }
                    }
                }
                div { class: "mt-1 truncate text-xs text-muted-foreground", "{space.profile}" }
            }
            div { class: "ml-3 flex shrink-0 items-center gap-2",
                div { class: "text-xs text-muted-foreground", "{tab_label}" }
                if deletable {
                    button {
                        class: "flex h-7 w-7 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground",
                        title: "Delete space",
                        "aria-label": "Delete space",
                        onclick: move |e| {
                            e.stop_propagation();
                            emit_command("delete", Some(delete_id.clone()), None);
                        },
                        span { class: "text-base leading-none", "\u{00d7}" }
                    }
                }
            }
        }
    }
}
