#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_processes::event::*;
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener, use_theme};

#[component]
pub fn App() -> Element {
    use_theme();
    let mut state = use_signal(|| ProcessesListEvent {
        connected: false,
        processes: Vec::new(),
    });
    let mut search = use_signal(String::new);

    let _listener =
        use_event_listener::<ProcessesListEvent, _>(PROCESSES_LIST_EVENT, move |event| {
            state.set(event);
        });

    let data = state.read();
    let query = search.read().to_lowercase();
    let filtered: Vec<&ProcessEntry> = data
        .processes
        .iter()
        .filter(|p| {
            if query.is_empty() {
                return true;
            }
            p.id.to_lowercase().contains(&query)
                || p.shell.to_lowercase().contains(&query)
                || p.cwd.to_lowercase().contains(&query)
                || p.pid.to_string().contains(&query)
        })
        .collect();

    let has_processes = !data.processes.is_empty();
    let process_count = data.processes.len();

    rsx! {
        div { class: "flex h-full flex-col bg-background p-4 overflow-auto",
            // Header
            div { class: "mb-3 flex items-center justify-between",
                div { class: "flex items-center gap-3",
                    div { class: "flex items-center gap-2 text-foreground",
                        ServiceIcon {}
                        h1 { class: "text-lg font-semibold", "Background Services" }
                    }
                    StatusBadge { connected: data.connected }
                    if has_processes {
                        {
                            let label = if process_count == 1 {
                                format!("{process_count} process")
                            } else {
                                format!("{process_count} processes")
                            };
                            rsx! { span { class: "text-xs text-muted-foreground", "{label}" } }
                        }
                    }
                }
                if has_processes {
                    button {
                        class: "rounded bg-red-500/10 px-2.5 py-1 text-xs text-red-400 hover:bg-red-500/20 transition-colors",
                        onclick: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            let _ = try_cef_emit_serde(&ProcessKillAllEvent { kill_all: true });
                        },
                        "Kill All"
                    }
                }
            }

            if !data.connected {
                div { class: "flex flex-1 items-center justify-center",
                    div { class: "text-center text-muted-foreground",
                        p { class: "text-sm", "Service is not running" }
                        p { class: "mt-1 text-xs opacity-60",
                            "Start with: "
                            code { class: "rounded bg-muted px-1.5 py-0.5 font-mono text-xs", "Vmux service" }
                        }
                    }
                }
            } else if !has_processes {
                div { class: "flex flex-1 items-center justify-center",
                    p { class: "text-sm text-muted-foreground", "No active processes" }
                }
            } else {
                // Search filter
                div { class: "mb-3",
                    input {
                        class: "w-full rounded-md border border-border bg-muted/50 px-3 py-1.5 text-sm text-foreground placeholder-muted-foreground outline-none focus:border-foreground/30",
                        r#type: "text",
                        placeholder: "Filter processes...",
                        value: "{search}",
                        oninput: move |e: Event<FormData>| search.set(e.value()),
                    }
                }

                if filtered.is_empty() {
                    div { class: "flex flex-1 items-center justify-center",
                        p { class: "text-sm text-muted-foreground", "No matching processes" }
                    }
                } else {
                    div { class: "flex flex-col gap-3",
                        for process in filtered.iter() {
                            ProcessCard { key: "{process.id}", process: (*process).clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ServiceIcon() -> Element {
    // lucide `server` icon — https://lucide.dev/icons/server
    rsx! {
        svg {
            width: "20",
            height: "20",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            rect { width: "20", height: "8", x: "2", y: "2", rx: "2" }
            rect { width: "20", height: "8", x: "2", y: "14", rx: "2" }
            line { x1: "6", x2: "6.01", y1: "6", y2: "6" }
            line { x1: "6", x2: "6.01", y1: "18", y2: "18" }
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
fn ProcessCard(process: ProcessEntry) -> Element {
    let uptime = format_uptime(process.uptime_secs);
    let id_short = if process.id.len() > 8 {
        &process.id[..8]
    } else {
        &process.id
    };
    let shell_name = process
        .shell
        .rsplit('/')
        .next()
        .unwrap_or(&process.shell)
        .to_string();

    let nav_id = process.id.clone();
    let kill_id = process.id.clone();

    let onclick = move |_| {
        let _ = try_cef_emit_serde(&ProcessNavigateEvent {
            process_id: nav_id.clone(),
            navigate: true,
        });
    };

    let onkill = move |e: Event<MouseData>| {
        e.stop_propagation();
        let _ = try_cef_emit_serde(&ProcessKillEvent {
            process_id: kill_id.clone(),
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
                    if process.attached {
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
                MetaRow { label: "PID", value: process.pid.to_string() }
                MetaRow { label: "Size", value: format!("{}x{}", process.cols, process.rows) }
                if !process.cwd.is_empty() {
                    MetaRow { label: "CWD", value: process.cwd.clone() }
                }
                MetaRow { label: "Shell", value: process.shell.clone() }
            }

            // Terminal preview
            if !process.preview_lines.is_empty() {
                div { class: "mt-2 rounded bg-muted/50 p-2 font-mono text-xs leading-tight text-muted-foreground",
                    for line in process.preview_lines.iter() {
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
