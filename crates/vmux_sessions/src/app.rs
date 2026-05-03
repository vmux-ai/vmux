#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_sessions::event::{
    SESSIONS_LIST_EVENT, SessionCommandEvent, SessionRow, SessionsListEvent,
};
use vmux_sessions::model::DEFAULT_SESSION_ID;
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener, use_theme};
use wasm_bindgen::JsCast;

fn emit_command(command: &str, session_id: Option<String>, name: Option<String>) {
    let _ = try_cef_emit_serde(&SessionCommandEvent {
        command: command.to_string(),
        session_id,
        name,
    });
}

#[component]
pub fn App() -> Element {
    use_theme();
    let mut state = use_signal(SessionsListEvent::default);
    let mut selected = use_signal(|| 0usize);

    let _listener = use_event_listener::<SessionsListEvent, _>(SESSIONS_LIST_EVENT, move |data| {
        selected.set(0);
        state.set(data);
    });

    use_effect(move || {
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("sessions-root"))
            && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = html.focus();
        }
    });

    let sessions = state.read().sessions.clone();
    let count = sessions.len();
    let sel = selected().min(count.saturating_sub(1));
    let active_name = sessions
        .iter()
        .find(|session| session.is_active)
        .map(|session| session.name.clone())
        .unwrap_or_else(|| "Default".to_string());
    let selected_session_id = sessions.get(sel).map(|session| session.id.clone());
    let selected_session_deletable = sessions
        .get(sel)
        .is_some_and(|session| session.id != DEFAULT_SESSION_ID);

    rsx! {
        div {
            id: "sessions-root",
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
                    if let Some(id) = selected_session_id.clone() {
                        emit_command("attach", Some(id), None);
                    }
                } else if e.key() == Key::Delete || e.key() == Key::Backspace {
                    e.prevent_default();
                    if selected_session_deletable
                        && let Some(id) = selected_session_id.clone()
                    {
                        emit_command("delete", Some(id), None);
                    }
                }
            },
            div { class: "flex items-center justify-between border-b border-border px-5 py-4",
                div { class: "min-w-0",
                    h1 { class: "text-lg font-semibold", "Sessions" }
                    div { class: "mt-1 truncate text-xs text-muted-foreground", "{active_name}" }
                }
                button {
                    class: "rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground transition-colors hover:border-foreground/30 hover:bg-muted",
                    onclick: move |_| {
                        emit_command("new", None, Some(format!("Session {}", count + 1)));
                    },
                    "New"
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-3",
                if sessions.is_empty() {
                    div { class: "flex h-full items-center justify-center text-sm text-muted-foreground", "No sessions" }
                } else {
                    div { class: "flex flex-col gap-2",
                        for (index, session) in sessions.iter().enumerate() {
                            SessionRowView {
                                key: "{session.id}",
                                session: session.clone(),
                                selected: index == sel,
                                deletable: session.id != DEFAULT_SESSION_ID,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionRowView(session: SessionRow, selected: bool, deletable: bool) -> Element {
    let nav_id = session.id.clone();
    let delete_id = session.id.clone();
    let class = if selected {
        "flex cursor-pointer items-center justify-between rounded-lg border border-foreground/30 bg-muted px-3 py-3"
    } else {
        "flex cursor-pointer items-center justify-between rounded-lg border border-border bg-card px-3 py-3 transition-colors hover:border-foreground/30 hover:bg-muted/60"
    };
    let tab_label = if session.tab_count == 1 {
        "1 tab".to_string()
    } else {
        format!("{} tabs", session.tab_count)
    };

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| {
                emit_command("attach", Some(nav_id.clone()), None);
            },
            div { class: "min-w-0",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "truncate text-sm font-medium text-foreground", "{session.name}" }
                    if session.is_active {
                        span { class: "rounded-full bg-blue-500/15 px-2 py-0.5 text-xs text-blue-300", "active" }
                    }
                }
                div { class: "mt-1 truncate text-xs text-muted-foreground", "{session.profile}" }
            }
            div { class: "ml-3 flex shrink-0 items-center gap-2",
                div { class: "text-xs text-muted-foreground", "{tab_label}" }
                if deletable {
                    button {
                        class: "flex h-7 w-7 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground",
                        title: "Delete session",
                        "aria-label": "Delete session",
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
