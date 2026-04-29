#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_sessions::event::*;
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener, use_theme};

#[component]
pub fn App() -> Element {
    use_theme();
    let mut state = use_signal(|| SessionsListEvent {
        connected: false,
        sessions: Vec::new(),
    });
    let mut search = use_signal(String::new);

    let _listener = use_event_listener::<SessionsListEvent, _>(SESSIONS_LIST_EVENT, move |event| {
        state.set(event);
    });

    let data = state.read();
    let query = search.read().to_lowercase();
    let filtered: Vec<&SessionEntry> = data
        .sessions
        .iter()
        .filter(|s| {
            if query.is_empty() {
                return true;
            }
            s.id.to_lowercase().contains(&query)
                || s.shell.to_lowercase().contains(&query)
                || s.cwd.to_lowercase().contains(&query)
                || s.pid.to_string().contains(&query)
        })
        .collect();

    let has_sessions = !data.sessions.is_empty();
    let session_count = data.sessions.len();

    rsx! {
        div { class: "flex h-full flex-col bg-background p-4 overflow-auto",
            // Header
            div { class: "mb-3 flex items-center justify-between",
                div { class: "flex items-center gap-3",
                    h1 { class: "text-lg font-semibold text-foreground", "Daemon Sessions" }
                    StatusBadge { connected: data.connected }
                    if has_sessions {
                        {
                            let label = if session_count == 1 {
                                format!("{session_count} session")
                            } else {
                                format!("{session_count} sessions")
                            };
                            rsx! { span { class: "text-xs text-muted-foreground", "{label}" } }
                        }
                    }
                }
                if has_sessions {
                    button {
                        class: "rounded bg-red-500/10 px-2.5 py-1 text-xs text-red-400 hover:bg-red-500/20 transition-colors",
                        onclick: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            let _ = try_cef_emit_serde(&SessionKillAllEvent { kill_all: true });
                        },
                        "Kill All"
                    }
                }
            }

            if !data.connected {
                div { class: "flex flex-1 items-center justify-center",
                    div { class: "text-center text-muted-foreground",
                        p { class: "text-sm", "Daemon is not running" }
                        p { class: "mt-1 text-xs opacity-60",
                            "Start with: "
                            code { class: "rounded bg-muted px-1.5 py-0.5 font-mono text-xs", "Vmux daemon" }
                        }
                    }
                }
            } else if !has_sessions {
                div { class: "flex flex-1 items-center justify-center",
                    p { class: "text-sm text-muted-foreground", "No active sessions" }
                }
            } else {
                // Search filter
                div { class: "mb-3",
                    input {
                        class: "w-full rounded-md border border-border bg-muted/50 px-3 py-1.5 text-sm text-foreground placeholder-muted-foreground outline-none focus:border-foreground/30",
                        r#type: "text",
                        placeholder: "Filter sessions...",
                        value: "{search}",
                        oninput: move |e: Event<FormData>| search.set(e.value()),
                    }
                }

                if filtered.is_empty() {
                    div { class: "flex flex-1 items-center justify-center",
                        p { class: "text-sm text-muted-foreground", "No matching sessions" }
                    }
                } else {
                    div { class: "flex flex-col gap-3",
                        for session in filtered.iter() {
                            SessionCard { key: "{session.id}", session: (*session).clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatusBadge(connected: bool) -> Element {
    let (color, text) = if connected {
        ("bg-green-500", "Connected")
    } else {
        ("bg-red-500", "Disconnected")
    };

    rsx! {
        div { class: "flex items-center gap-1.5 rounded-full bg-muted px-2.5 py-0.5",
            div { class: "h-2 w-2 rounded-full {color}" }
            span { class: "text-xs text-muted-foreground", "{text}" }
        }
    }
}

#[component]
fn SessionCard(session: SessionEntry) -> Element {
    let uptime = format_uptime(session.uptime_secs);
    let id_short = if session.id.len() > 8 {
        &session.id[..8]
    } else {
        &session.id
    };
    let shell_name = session
        .shell
        .rsplit('/')
        .next()
        .unwrap_or(&session.shell)
        .to_string();

    let nav_id = session.id.clone();
    let kill_id = session.id.clone();

    let onclick = move |_| {
        let _ = try_cef_emit_serde(&SessionNavigateEvent {
            session_id: nav_id.clone(),
            navigate: true,
        });
    };

    let onkill = move |e: Event<MouseData>| {
        e.stop_propagation();
        let _ = try_cef_emit_serde(&SessionKillEvent {
            session_id: kill_id.clone(),
            kill: true,
        });
    };

    rsx! {
        div {
            class: "rounded-lg border border-border bg-card p-3 cursor-pointer hover:border-foreground/30 transition-colors",
            onclick,

            // Row 1: ID + badges + uptime + kill
            div { class: "mb-2 flex items-center justify-between",
                div { class: "flex items-center gap-2",
                    code { class: "rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-foreground",
                        "{id_short}"
                    }
                    span { class: "rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground",
                        "{shell_name}"
                    }
                    if session.attached {
                        span { class: "rounded-full bg-blue-500/20 px-2 py-0.5 text-xs text-blue-400",
                            "attached"
                        }
                    }
                }
                div { class: "flex items-center gap-2",
                    span { class: "text-xs text-muted-foreground", "{uptime}" }
                    button {
                        class: "rounded px-1.5 py-0.5 text-xs text-red-400 hover:bg-red-500/20 transition-colors",
                        onclick: onkill,
                        "Kill"
                    }
                }
            }

            // Row 2: metadata grid
            div { class: "grid grid-cols-2 gap-x-4 gap-y-1 text-xs",
                MetaRow { label: "PID", value: session.pid.to_string() }
                MetaRow { label: "Size", value: format!("{}x{}", session.cols, session.rows) }
                if !session.cwd.is_empty() {
                    MetaRow { label: "CWD", value: session.cwd.clone() }
                }
                MetaRow { label: "Shell", value: session.shell.clone() }
            }

            // Terminal preview
            if !session.preview_lines.is_empty() {
                div { class: "mt-2 rounded bg-muted/50 p-2 font-mono text-xs leading-tight text-muted-foreground",
                    for line in session.preview_lines.iter() {
                        div { class: "truncate whitespace-pre", "{line.text}" }
                    }
                }
            }
        }
    }
}

#[component]
fn MetaRow(label: String, value: String) -> Element {
    rsx! {
        div { class: "flex gap-1 min-w-0",
            span { class: "shrink-0 text-muted-foreground", "{label}:" }
            span { class: "truncate text-foreground", "{value}" }
        }
    }
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}
