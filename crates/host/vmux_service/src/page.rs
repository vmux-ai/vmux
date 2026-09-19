#![allow(non_snake_case)]

use std::collections::VecDeque;

use crate::event::*;
use dioxus::prelude::*;
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerTone,
};
use vmux_ui::hooks::{send, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(|| ProcessesListEvent {
        connected: false,
        processes: Vec::new(),
    });
    let mut history = use_signal(ServiceHistory::default);
    let _processes = use_listener::<ProcessesListEvent, _>(PROCESSES_LIST_EVENT, move |event| {
        history.write().push(&event);
        state.set(event);
    });
    let mut search = use_signal(String::new);

    let data = state.read();
    let query = search.read().to_lowercase();
    let mut filtered: Vec<ProcessEntry> = data
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
        .cloned()
        .collect();
    filtered.sort_by(|left, right| {
        right
            .cpu_percent
            .total_cmp(&left.cpu_percent)
            .then_with(|| right.mem_bytes.cmp(&left.mem_bytes))
            .then_with(|| left.pid.cmp(&right.pid))
    });

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
                class: "mx-auto flex w-full max-w-7xl flex-col gap-3".to_string(),
                if !data.connected && !has_processes {
                    ManagerEmpty { title: translate("services-not-running"), detail: empty_detail }
                } else if !has_processes {
                    ManagerEmpty { title: translate("services-empty"), detail: String::new() }
                } else if filtered.is_empty() {
                    ManagerEmpty { title: translate("services-no-match"), detail: String::new() }
                } else {
                    ServiceDashboard {
                        processes: filtered,
                        history: history.read().clone(),
                    }
                }
            }
        }
    }
}

#[derive(Clone, Default, PartialEq)]
struct ServiceHistory {
    cpu: VecDeque<f32>,
    memory: VecDeque<f32>,
}

impl ServiceHistory {
    const LIMIT: usize = 72;

    fn push(&mut self, event: &ProcessesListEvent) {
        let cpu = event
            .processes
            .iter()
            .map(|process| process.cpu_percent)
            .sum();
        let memory = event
            .processes
            .iter()
            .map(|process| process.mem_bytes as f64)
            .sum::<f64>()
            / (1024.0 * 1024.0);
        Self::push_sample(&mut self.cpu, cpu);
        Self::push_sample(&mut self.memory, memory as f32);
    }

    fn push_sample(samples: &mut VecDeque<f32>, value: f32) {
        samples.push_back(value.max(0.0));
        while samples.len() > Self::LIMIT {
            samples.pop_front();
        }
    }
}

#[component]
fn ServiceDashboard(processes: Vec<ProcessEntry>, history: ServiceHistory) -> Element {
    let total_cpu = processes
        .iter()
        .map(|process| process.cpu_percent)
        .sum::<f32>();
    let total_memory = processes
        .iter()
        .map(|process| process.mem_bytes)
        .sum::<u64>();
    let peak_cpu = history.cpu.iter().copied().fold(0.0_f32, f32::max);
    let peak_memory_mb = history.memory.iter().copied().fold(0.0_f32, f32::max);
    let memory_mb = total_memory as f64 / (1024.0 * 1024.0);
    rsx! {
        div { class: "grid min-h-0 gap-3 xl:grid-cols-2",
            UsageChart {
                class: "xl:col-span-2".to_string(),
                label: "CPU".to_string(),
                value: format!("{total_cpu:.1}%"),
                peak: format!("↑ {peak_cpu:.1}%"),
                samples: history.cpu.iter().copied().collect(),
                floor: 100.0,
                tone: UsageTone::Cpu,
            }
            UsageChart {
                class: String::new(),
                label: translate("services-memory"),
                value: format_mem(total_memory),
                peak: format!("↑ {:.0} MB", peak_memory_mb.max(memory_mb as f32)),
                samples: history.memory.iter().copied().collect(),
                floor: 128.0,
                tone: UsageTone::Memory,
            }
            ProcessTable { processes }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum UsageTone {
    Cpu,
    Memory,
}

impl UsageTone {
    fn line_class(self) -> &'static str {
        match self {
            Self::Cpu => "text-sky-400",
            Self::Memory => "text-violet-400",
        }
    }

    fn fill(self) -> &'static str {
        match self {
            Self::Cpu => "rgba(56,189,248,0.12)",
            Self::Memory => "rgba(167,139,250,0.12)",
        }
    }
}

#[component]
fn UsageChart(
    class: String,
    label: String,
    value: String,
    peak: String,
    samples: Vec<f32>,
    floor: f32,
    tone: UsageTone,
) -> Element {
    let graph = Sparkline::of(&samples, floor);
    let line_class = tone.line_class();
    let fill = tone.fill();
    rsx! {
        section { class: "min-w-0 overflow-hidden rounded-xl bg-foreground/[0.025] ring-1 ring-inset ring-foreground/10 {class}",
            div { class: "flex h-9 items-center justify-between border-b border-foreground/[0.07] px-3",
                div { class: "flex items-baseline gap-2",
                    h2 { class: "font-mono text-[10px] font-semibold uppercase tracking-[0.12em] text-foreground", "{label}" }
                    span { class: "font-mono text-xs font-semibold {line_class}", "{value}" }
                }
                span { class: "font-mono text-[9px] text-muted-foreground", "{peak}" }
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
                    polygon { points: "{graph.area}", fill }
                    polyline { points: "{graph.line}", fill: "none", stroke: "currentColor", stroke_width: "1.35", vector_effect: "non-scaling-stroke" }
                }
            }
        }
    }
}

struct Sparkline {
    line: String,
    area: String,
}

impl Sparkline {
    fn of(samples: &[f32], floor: f32) -> Self {
        let samples = if samples.is_empty() {
            vec![0.0, 0.0]
        } else if samples.len() == 1 {
            vec![samples[0], samples[0]]
        } else {
            samples.to_vec()
        };
        let ceiling = samples.iter().copied().fold(floor, f32::max).max(1.0);
        let last = (samples.len() - 1) as f32;
        let mut points = Vec::with_capacity(samples.len());
        for (index, sample) in samples.iter().enumerate() {
            let x = index as f32 / last * 100.0;
            let y = 39.0 - (sample / ceiling).clamp(0.0, 1.0) * 37.0;
            points.push(format!("{x:.2},{y:.2}"));
        }
        let line = points.join(" ");
        let area = format!("0,40 {line} 100,40");
        Self { line, area }
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
fn ProcessRow(process: ProcessEntry) -> Element {
    let uptime = format_uptime(process.uptime_secs);
    let shell_name = process
        .shell
        .rsplit('/')
        .next()
        .unwrap_or(&process.shell)
        .to_string();
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
            div { class: "flex min-w-0 items-center gap-2",
                div { class: if process.attached {
                        "flex size-6 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-inset ring-primary/20"
                    } else {
                        "flex size-6 shrink-0 items-center justify-center rounded-md bg-foreground/[0.05] text-muted-foreground ring-1 ring-inset ring-foreground/[0.08]"
                    },
                    ServiceIcon {}
                }
                div { class: "min-w-0 flex-1",
                    div { class: "flex min-w-0 items-center gap-1.5",
                        span { class: "min-w-0 truncate font-mono text-[10px] font-semibold text-foreground", "{shell_name}" }
                        if process.attached {
                            span { class: "size-1.5 shrink-0 rounded-full bg-primary shadow-[0_0_7px_color-mix(in_oklab,var(--primary)_60%,transparent)]" }
                        }
                    }
                    div { class: "truncate font-mono text-[8px] text-muted-foreground/65", title: "{process.cwd}", "{process.cwd}" }
                }
            }
            span { class: if process.cpu_percent >= 25.0 { "text-right font-mono text-[10px] font-semibold tabular-nums text-amber-400" } else { "text-right font-mono text-[10px] tabular-nums text-foreground" }, "{process.cpu_percent:.1}" }
            span { class: "text-right font-mono text-[9px] tabular-nums text-foreground", {format_mem(process.mem_bytes)} }
            span { class: "hidden text-right font-mono text-[9px] tabular-nums text-muted-foreground sm:block", "{uptime}" }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_history_keeps_the_newest_samples() {
        let mut history = ServiceHistory::default();
        for value in 0..ServiceHistory::LIMIT + 3 {
            ServiceHistory::push_sample(&mut history.cpu, value as f32);
        }

        assert_eq!(history.cpu.len(), ServiceHistory::LIMIT);
        assert_eq!(history.cpu.front(), Some(&3.0));
    }

    #[test]
    fn sparkline_scales_against_a_floor() {
        let graph = Sparkline::of(&[0.0, 50.0, 100.0], 100.0);

        assert_eq!(graph.line, "0.00,39.00 50.00,20.50 100.00,2.00");
        assert!(graph.area.starts_with("0,40 "));
    }
}
