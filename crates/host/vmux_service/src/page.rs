#![allow(non_snake_case)]

use crate::event::*;
use dioxus::prelude::*;
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerTone,
};
use vmux_ui::hooks::{send, use_event, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_event::<ProcessesListEvent>(PROCESSES_LIST_EVENT, || ProcessesListEvent {
        connected: false,
        processes: Vec::new(),
    });
    let mut search = use_signal(String::new);

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
    let has_managed_processes = data.processes.iter().any(|process| process.managed);
    let process_count = data.processes.len();

    let empty_detail = format!(
        "{} {}",
        translate("services-start-with"),
        translate("services-command")
    );

    rsx! {
        ManagerPage {
            ManagerHeader {
                title: translate("services-title"),
                count: process_count,
                search_value: search(),
                search_placeholder: translate("services-filter"),
                onsearch: move |event: FormEvent| search.set(event.value()),
                onkeydown: None,
                actions: rsx! {
                    StatusBadge { connected: data.connected }
                    if has_managed_processes {
                        ManagerButton {
                            variant: ManagerButtonVariant::Danger,
                            onclick: move |event: Event<MouseData>| {
                                event.stop_propagation();
                                let _ = send(&ProcessKillAllEvent { kill_all: true });
                            },
                            {translate("services-kill-all")}
                        }
                    }
                },
            }
            ManagerList {
                class: "mx-auto grid w-full max-w-7xl grid-cols-1 items-start gap-3 md:grid-cols-2 2xl:grid-cols-3".to_string(),
                if !data.connected && !has_processes {
                    ManagerEmpty { title: translate("services-not-running"), detail: empty_detail }
                } else if !has_processes {
                    ManagerEmpty { title: translate("services-empty"), detail: String::new() }
                } else if filtered.is_empty() {
                    ManagerEmpty { title: translate("services-no-match"), detail: String::new() }
                } else {
                    for process in filtered.iter() {
                        ProcessCard { key: "{process.id}", process: (*process).clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn ServiceIcon() -> Element {
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
    let (tone, color, text) = if connected {
        (
            ManagerTone::Green,
            "bg-success",
            translate("services-connected"),
        )
    } else {
        (
            ManagerTone::Amber,
            "bg-amber-400",
            translate("services-disconnected"),
        )
    };

    rsx! {
        ManagerBadge { tone,
            span { class: "flex items-center gap-1.5",
                span { class: "size-1.5 rounded-full {color}" }
                "{text}"
            }
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

    let managed = process.managed;
    let nav_id = process.id.clone();
    let kill_id = process.id.clone();
    let identifier = format!("PID {}", process.pid);

    let onclick = move |_| {
        if !managed {
            return;
        }
        let _ = send(&ProcessNavigateEvent {
            process_id: nav_id.clone(),
            navigate: true,
        });
    };

    let onkill = move |e: Event<MouseData>| {
        e.stop_propagation();
        let _ = send(&ProcessKillEvent {
            process_id: kill_id.clone(),
            kill: true,
        });
    };

    rsx! {
        article {
            class: if process.attached {
                "group min-w-0 cursor-pointer overflow-hidden rounded-xl bg-primary/[0.07] ring-1 ring-inset ring-primary/25 backdrop-blur-xl transition-colors hover:bg-primary/[0.11]"
            } else if managed {
                "group min-w-0 cursor-pointer overflow-hidden rounded-xl bg-foreground/[0.035] ring-1 ring-inset ring-foreground/10 backdrop-blur-xl transition-colors hover:bg-foreground/[0.07]"
            } else {
                "group min-w-0 overflow-hidden rounded-xl bg-foreground/[0.035] ring-1 ring-inset ring-foreground/10 backdrop-blur-xl transition-colors hover:bg-foreground/[0.07]"
            },
            onclick,
            div { class: "flex items-center gap-3 border-b border-foreground/[0.07] px-3 py-2.5",
                div { class: if process.attached {
                        "flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary ring-1 ring-inset ring-primary/20"
                    } else {
                        "flex size-8 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.06] text-muted-foreground ring-1 ring-inset ring-foreground/10"
                    },
                    ServiceIcon {}
                }
                div { class: "min-w-0 flex-1",
                    div { class: "flex min-w-0 items-center gap-2",
                        span { class: "min-w-0 flex-1 truncate font-mono text-xs font-semibold text-foreground", "{shell_name}" }
                        if process.attached { span { class: "size-2 shrink-0 rounded-full bg-primary shadow-[0_0_8px_color-mix(in_oklab,var(--primary)_65%,transparent)]" } }
                    }
                    div { class: "mt-0.5 min-w-0 truncate font-mono text-[9px] text-muted-foreground", title: "{process.cwd}", "{process.cwd}" }
                }
                if managed {
                    div { class: "shrink-0",
                        ManagerButton {
                            variant: ManagerButtonVariant::Danger,
                            onclick: onkill,
                            {translate("services-kill")}
                        }
                    }
                }
            }
            div { class: "grid grid-cols-4 gap-px bg-foreground/[0.07]",
                ProcessMetric { label: "CPU".to_string(), value: format!("{:.0}%", process.cpu_percent), tone: ManagerTone::Amber }
                ProcessMetric { label: translate("services-memory"), value: format_mem(process.mem_bytes), tone: ManagerTone::Neutral }
                ProcessMetric { label: translate("services-uptime"), value: uptime, tone: ManagerTone::Neutral }
                ProcessMetric { label: if managed { translate("services-size") } else { "PID".to_string() }, value: if managed { format!("{}×{}", process.cols, process.rows) } else { process.pid.to_string() }, tone: ManagerTone::Neutral }
            }
            if !process.preview_lines.is_empty() {
                div { class: "min-h-24 bg-zinc-950 px-3 py-2 font-mono text-[10px] leading-4 text-zinc-400",
                    for line in process.preview_lines.iter().take(6) {
                        div { class: "truncate whitespace-pre", "{line.text}" }
                    }
                }
            } else {
                div { class: "flex min-h-24 items-center justify-center bg-zinc-950/80 font-mono text-[10px] text-zinc-600", "{identifier} · {id_short}" }
            }
        }
    }
}

#[component]
fn ProcessMetric(label: String, value: String, tone: ManagerTone) -> Element {
    let color = match tone {
        ManagerTone::Amber => "text-amber-400",
        ManagerTone::Green => "text-success",
        _ => "text-foreground",
    };
    rsx! {
        div { class: "flex min-w-0 flex-col bg-card/70 px-2 py-1.5",
            span { class: "truncate text-[8px] font-medium uppercase tracking-wide text-muted-foreground/65", "{label}" }
            span { class: "truncate font-mono text-[10px] font-semibold {color}", "{value}" }
        }
    }
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        translate_with(
            "services-uptime-seconds",
            &[("seconds", TranslationValue::Number(secs as i64))],
        )
    } else if secs < 3600 {
        translate_with(
            "services-uptime-minutes",
            &[
                ("minutes", TranslationValue::Number((secs / 60) as i64)),
                ("seconds", TranslationValue::Number((secs % 60) as i64)),
            ],
        )
    } else if secs < 86400 {
        translate_with(
            "services-uptime-hours",
            &[
                ("hours", TranslationValue::Number((secs / 3600) as i64)),
                (
                    "minutes",
                    TranslationValue::Number(((secs % 3600) / 60) as i64),
                ),
            ],
        )
    } else {
        translate_with(
            "services-uptime-days",
            &[
                ("days", TranslationValue::Number((secs / 86400) as i64)),
                (
                    "hours",
                    TranslationValue::Number(((secs % 86400) / 3600) as i64),
                ),
            ],
        )
    }
}
