#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_history::event::HistoryEntry;
use vmux_ui::hooks::use_theme;

#[component]
pub fn App() -> Element {
    use_theme();
    let entries: Signal<Vec<HistoryEntry>> = use_signal(Vec::new);
    let mut query: Signal<String> = use_signal(String::new);

    let entries_for_render = entries.read().clone();

    rsx! {
        div { class: "flex flex-col h-screen bg-background text-foreground",
            header { class: "p-3 border-b border-border flex gap-2 items-center",
                input {
                    class: "flex-1 bg-muted px-3 py-2 rounded text-sm outline-none",
                    placeholder: "Search history",
                    value: "{query.read()}",
                    oninput: move |e| query.set(e.value()),
                }
                button {
                    class: "px-3 py-2 text-xs bg-destructive text-destructive-foreground rounded",
                    "Clear all"
                }
            }
            main { class: "flex-1 overflow-y-auto p-3 text-sm",
                {render_timeline(&entries_for_render)}
                div { id: "infinite-scroll-sentinel", class: "h-4" }
            }
        }
    }
}

fn render_timeline(entries: &[HistoryEntry]) -> Element {
    let groups = group_by_day(entries);
    rsx! {
        for (label, group) in groups {
            div { class: "text-xs text-muted-foreground uppercase mt-4 mb-1", "{label}" }
            for entry in group {
                div {
                    class: "flex items-center gap-2 py-1 border-b border-border hover:bg-muted group",
                    span { class: "text-xs text-muted-foreground w-12", "{format_time(entry.visit_created_at)}" }
                    img {
                        class: "w-4 h-4",
                        src: "{entry.favicon_url}",
                    }
                    span { class: "flex-1 truncate",
                        if entry.title.is_empty() { "{entry.url}" } else { "{entry.title}" }
                    }
                    button {
                        class: "opacity-0 group-hover:opacity-100 text-xs text-muted-foreground hover:text-destructive px-2",
                        "\u{00d7}"
                    }
                }
            }
        }
    }
}

fn group_by_day(entries: &[HistoryEntry]) -> Vec<(String, Vec<HistoryEntry>)> {
    let mut out: Vec<(String, Vec<HistoryEntry>)> = Vec::new();
    let mut current_day: Option<i64> = None;
    let now_day = now_millis_wasm() / 86_400_000;
    for e in entries {
        let day = e.visit_created_at / 86_400_000;
        if current_day != Some(day) {
            let label = match now_day - day {
                0 => "Today".to_string(),
                1 => "Yesterday".to_string(),
                d if d < 7 => format!("{} days ago", d),
                _ => format!("Day -{}", now_day - day),
            };
            out.push((label, Vec::new()));
            current_day = Some(day);
        }
        out.last_mut().unwrap().1.push(e.clone());
    }
    out
}

fn now_millis_wasm() -> i64 {
    js_sys::Date::now() as i64
}

fn format_time(ms: i64) -> String {
    let total_sec = ms / 1000;
    let h = (total_sec % 86400) / 3600;
    let m = (total_sec % 3600) / 60;
    format!("{:02}:{:02}", h, m)
}
