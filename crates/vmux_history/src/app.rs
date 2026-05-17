#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_history::event::{
    HISTORY_QUERY_RESPONSE_EVENT, HistoryClearAllRequest, HistoryDeleteRequest, HistoryEntry,
    HistoryOpenRequest, HistoryQueryRequest, HistoryQueryResponse,
};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

fn emit_query(query: &str, offset: u32, request_id: u64) {
    let req = HistoryQueryRequest {
        query: if query.is_empty() {
            None
        } else {
            Some(query.to_string())
        },
        offset,
        limit: 50,
        request_id,
    };
    let _ = try_cef_bin_emit_rkyv(&req);
}

#[component]
pub fn App() -> Element {
    use_theme();
    let mut entries: Signal<Vec<HistoryEntry>> = use_signal(Vec::new);
    let mut query: Signal<String> = use_signal(String::new);
    let mut offset: Signal<u32> = use_signal(|| 0);
    let mut has_more: Signal<bool> = use_signal(|| true);
    let mut request_id: Signal<u64> = use_signal(|| 0);
    let mut last_reset_id: Signal<u64> = use_signal(|| 0);

    let _listener = use_bin_event_listener::<HistoryQueryResponse, _>(
        HISTORY_QUERY_RESPONSE_EVENT,
        move |resp: HistoryQueryResponse| {
            if resp.request_id < *last_reset_id.read() {
                return;
            }
            if resp.request_id == *last_reset_id.read() {
                entries.set(resp.entries);
            } else {
                entries.write().extend(resp.entries);
            }
            has_more.set(resp.has_more);
        },
    );

    use_effect(move || {
        request_id.set(1);
        last_reset_id.set(1);
        emit_query("", 0, 1);
    });

    use_effect(move || {
        let load_more =
            Closure::<dyn FnMut(js_sys::Array)>::new(move |entries_arr: js_sys::Array| {
                if entries_arr.length() == 0 {
                    return;
                }
                let entry: web_sys::IntersectionObserverEntry =
                    entries_arr.get(0).dyn_into().unwrap();
                if !entry.is_intersecting() {
                    return;
                }
                if !*has_more.read() {
                    return;
                }
                if entries.read().is_empty() {
                    return;
                }
                let new_offset = *offset.read() + 50;
                offset.set(new_offset);
                let new_id = *request_id.read() + 1;
                request_id.set(new_id);
                emit_query(&query.read(), new_offset, new_id);
            });

        let window = web_sys::window().expect("window");
        let document = window.document().expect("document");
        if let Some(target) = document.get_element_by_id("infinite-scroll-sentinel") {
            let cb: &js_sys::Function = load_more.as_ref().unchecked_ref();
            if let Ok(observer) = web_sys::IntersectionObserver::new(cb) {
                observer.observe(&target);
                load_more.forget();
            }
        }
    });

    let mut confirm_open: Signal<bool> = use_signal(|| false);

    let on_input = move |e: Event<FormData>| {
        query.set(e.value());
        let new_id = *request_id.read() + 1;
        request_id.set(new_id);
        offset.set(0);
        last_reset_id.set(new_id);
        emit_query(&query.read(), 0, new_id);
    };

    let groups = group_by_day(&entries.read());

    rsx! {
        div { class: "flex flex-col h-screen bg-background text-foreground",
            header { class: "p-3 border-b border-border flex gap-2 items-center",
                input {
                    class: "flex-1 bg-muted px-3 py-2 rounded text-sm outline-none",
                    placeholder: "Search history",
                    value: "{query.read()}",
                    oninput: on_input,
                }
                button {
                    class: "px-3 py-2 text-xs bg-destructive text-destructive-foreground rounded",
                    onclick: move |_| confirm_open.set(true),
                    "Clear all"
                }
            }
            main { class: "flex-1 overflow-y-auto p-3 text-sm",
                for (label, group) in groups {
                    div { class: "text-xs text-muted-foreground uppercase mt-4 mb-1", "{label}" }
                    for entry in group {
                        div {
                            class: "flex items-center gap-2 py-1 border-b border-border hover:bg-muted group cursor-pointer",
                            onclick: {
                                let url = entry.url.clone();
                                move |_| {
                                    let _ = try_cef_bin_emit_rkyv(&HistoryOpenRequest {
                                        url: url.clone(),
                                        in_new_stack: true,
                                    });
                                }
                            },
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
                                onclick: {
                                    let url_bits = entry.url_entity_bits;
                                    move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                        let _ = try_cef_bin_emit_rkyv(&HistoryDeleteRequest { url_entity_bits: url_bits });
                                        entries.write().retain(|x| x.url_entity_bits != url_bits);
                                    }
                                },
                                "\u{00d7}"
                            }
                        }
                    }
                }
                div { id: "infinite-scroll-sentinel", class: "h-4" }
            }
        }
        if *confirm_open.read() {
            div { class: "fixed inset-0 bg-black/80 flex items-center justify-center z-50",
                div { class: "bg-card border border-border p-6 rounded max-w-sm",
                    h3 { class: "text-lg mb-2", "Clear all history?" }
                    p { class: "text-sm text-muted-foreground mb-4", "This cannot be undone." }
                    div { class: "flex gap-2 justify-end",
                        button {
                            class: "px-3 py-1 text-sm bg-muted rounded",
                            onclick: move |_| confirm_open.set(false),
                            "Cancel"
                        }
                        button {
                            class: "px-3 py-1 text-sm bg-destructive text-destructive-foreground rounded",
                            onclick: move |_| {
                                let _ = try_cef_bin_emit_rkyv(&HistoryClearAllRequest);
                                entries.write().clear();
                                confirm_open.set(false);
                            },
                            "Clear all"
                        }
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
