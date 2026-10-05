#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::service::*;
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerTone,
};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

#[vmux_page::page(page = "process_monitor", component = Page)]
pub struct ProcessMonitorPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<ProcessesUiState>().state;
    let snapshot = state();
    let processes = snapshot.processes.clone();
    let connected = snapshot.connected;
    let has_processes = snapshot.total_count != 0;
    let has_managed_processes = snapshot.managed_count != 0;
    let process_count = snapshot.total_count as usize;

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
                search_value: snapshot.query.clone(),
                search_placeholder: translate("services-filter"),
                onsearch: move |event: FormEvent| {
                    let _ = send(&ProcessSearchRequest { query: event.value() });
                },
                onkeydown: None,
                actions: rsx! {
                    StatusBadge { connected }
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
                class: "mx-auto flex w-full max-w-7xl flex-col gap-3".to_string(),
                if !connected && !has_processes {
                    ManagerEmpty { title: translate("services-not-running"), detail: empty_detail }
                } else if !has_processes {
                    ManagerEmpty { title: translate("services-empty"), detail: String::new() }
                } else if processes.is_empty() {
                    ManagerEmpty { title: translate("services-no-match"), detail: String::new() }
                } else {
                    ServiceDashboard {
                        processes,
                        cpu: snapshot.cpu.clone(),
                        memory: snapshot.memory.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn ServiceDashboard(
    processes: Vec<ProcessEntry>,
    cpu: ProcessUsageUiState,
    memory: ProcessUsageUiState,
) -> Element {
    rsx! {
        div { class: "grid min-h-0 gap-3 xl:grid-cols-2",
            UsageChart {
                class: "xl:col-span-2".to_string(),
                usage: cpu,
                line_class: "text-sky-400".to_string(),
                fill: "rgba(56,189,248,0.12)".to_string(),
            }
            UsageChart {
                class: String::new(),
                usage: memory,
                line_class: "text-violet-400".to_string(),
                fill: "rgba(167,139,250,0.12)".to_string(),
            }
            ProcessTable { processes }
        }
    }
}

#[component]
fn UsageChart(
    class: String,
    usage: ProcessUsageUiState,
    line_class: String,
    fill: String,
) -> Element {
    rsx! {
        section { class: "min-w-0 overflow-hidden rounded-xl bg-foreground/[0.025] ring-1 ring-inset ring-foreground/10 {class}",
            div { class: "flex h-9 items-center justify-between border-b border-foreground/[0.07] px-3",
                div { class: "flex items-baseline gap-2",
                    h2 { class: "font-mono text-[10px] font-semibold uppercase tracking-[0.12em] text-foreground", "{usage.label}" }
                    span { class: "font-mono text-xs font-semibold {line_class}", "{usage.value}" }
                }
                span { class: "font-mono text-[9px] text-muted-foreground", "{usage.peak}" }
            }
            div { class: "relative h-36 bg-background/30 px-2 py-2",
                svg {
                    class: "h-full w-full overflow-visible {line_class}",
                    view_box: "0 0 100 40",
                    preserve_aspect_ratio: "none",
                    "aria-hidden": "true",
                    for y in [10, 20, 30] {
                        line { x1: "0", x2: "100", y1: "{y}", y2: "{y}", stroke: "currentColor", stroke_opacity: "0.08", stroke_width: "0.5" }
                    }
                    polygon { points: "{usage.area}", fill }
                    polyline { points: "{usage.line}", fill: "none", stroke: "currentColor", stroke_width: "1.35", vector_effect: "non-scaling-stroke" }
                }
            }
        }
    }
}

#[component]
fn ProcessTable(processes: Vec<ProcessEntry>) -> Element {
    rsx! {
        section { class: "min-w-0 overflow-hidden rounded-xl bg-foreground/[0.025] ring-1 ring-inset ring-foreground/10",
            div { class: "flex h-9 items-center justify-between border-b border-foreground/[0.07] px-3",
                h2 { class: "font-mono text-[10px] font-semibold uppercase tracking-[0.12em] text-foreground", {translate("services-title")} }
                span { class: "rounded-full bg-foreground/[0.06] px-2 font-mono text-[9px] text-muted-foreground", "{processes.len()}" }
            }
            div { class: "grid grid-cols-[3rem_minmax(0,1fr)_3.5rem_4.5rem_2rem] items-center gap-2 border-b border-foreground/[0.07] px-3 py-1.5 font-mono text-[8px] uppercase tracking-wide text-muted-foreground/65 sm:grid-cols-[3rem_minmax(0,1fr)_3.5rem_4.5rem_4.5rem_2rem]",
                span { "PID" }
                span { {translate("services-shell")} }
                span { class: "text-right", "CPU" }
                span { class: "text-right", {translate("services-memory")} }
                span { class: "hidden text-right sm:block", {translate("services-uptime")} }
                span {}
            }
            div { class: "max-h-72 overflow-y-auto",
                for process in processes {
                    ProcessRow { key: "{process.id}", process }
                }
            }
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
fn ProcessRow(process: ProcessEntry) -> Element {
    let managed = process.managed;
    let nav_id = process.id.clone();
    let kill_id = process.id.clone();
    let onclick = move |_| {
        if !managed {
            return;
        }
        let _ = send(&ProcessNavigateEvent {
            process_id: nav_id.clone(),
            navigate: true,
        });
    };
    let onkill = move |event: Event<MouseData>| {
        event.stop_propagation();
        let _ = send(&ProcessKillEvent {
            process_id: kill_id.clone(),
            kill: true,
        });
    };

    rsx! {
        div {
            class: if process.attached {
                "group grid min-w-0 cursor-pointer grid-cols-[3rem_minmax(0,1fr)_3.5rem_4.5rem_2rem] items-center gap-2 border-b border-primary/10 bg-primary/[0.07] px-3 py-2 text-left transition-colors last:border-0 hover:bg-primary/[0.12] sm:grid-cols-[3rem_minmax(0,1fr)_3.5rem_4.5rem_4.5rem_2rem]"
            } else if managed {
                "group grid min-w-0 cursor-pointer grid-cols-[3rem_minmax(0,1fr)_3.5rem_4.5rem_2rem] items-center gap-2 border-b border-foreground/[0.06] px-3 py-2 text-left transition-colors last:border-0 hover:bg-foreground/[0.055] sm:grid-cols-[3rem_minmax(0,1fr)_3.5rem_4.5rem_4.5rem_2rem]"
            } else {
                "group grid min-w-0 grid-cols-[3rem_minmax(0,1fr)_3.5rem_4.5rem_2rem] items-center gap-2 border-b border-foreground/[0.06] px-3 py-2 text-left transition-colors last:border-0 hover:bg-foreground/[0.035] sm:grid-cols-[3rem_minmax(0,1fr)_3.5rem_4.5rem_4.5rem_2rem]"
            },
            onclick,
            span { class: "font-mono text-[9px] tabular-nums text-muted-foreground", "{process.pid}" }
            div { class: "min-w-0",
                div { class: "flex min-w-0 items-center gap-1.5",
                    span { class: "min-w-0 truncate font-mono text-[10px] font-semibold text-foreground", title: "{process.shell}", "{process.shell_label}" }
                    if process.attached {
                        span { class: "size-1.5 shrink-0 rounded-full bg-primary shadow-[0_0_7px_color-mix(in_oklab,var(--primary)_60%,transparent)]" }
                    }
                }
                if let Some(cwd) = process.cwd_label {
                    div { class: "truncate font-mono text-[8px] text-muted-foreground/65", title: "{cwd}", "{cwd}" }
                }
            }
            span { class: if process.cpu_percent >= 25.0 { "text-right font-mono text-[10px] font-semibold tabular-nums text-amber-400" } else { "text-right font-mono text-[10px] tabular-nums text-foreground" }, "{process.cpu_label}" }
            span { class: "text-right font-mono text-[9px] tabular-nums text-foreground", "{process.memory_label}" }
            span { class: "hidden text-right font-mono text-[9px] tabular-nums text-muted-foreground sm:block", "{process.uptime_label}" }
            if managed {
                button {
                    r#type: "button",
                    class: "flex size-6 items-center justify-center rounded-md text-muted-foreground opacity-45 transition-colors hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100",
                    title: translate("services-kill"),
                    aria_label: translate("services-kill"),
                    onclick: onkill,
                    LineIconView { icon: LineIcon::Trash, class: "size-3" }
                }
            } else {
                span { class: "flex size-6 items-center justify-center text-muted-foreground/25",
                    LineIconView { icon: LineIcon::Minus, class: "size-3" }
                }
            }
        }
    }
}
