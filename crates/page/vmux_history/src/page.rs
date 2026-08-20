#![allow(non_snake_case)]

use crate::event::{
    HISTORY_CHANGED_EVENT, HISTORY_QUERY_RESPONSE_EVENT, HistoryChangedEvent,
    HistoryClearAllRequest, HistoryDeleteRequest, HistoryEntry, HistoryOpenRequest,
    HistoryQueryRequest, HistoryQueryResponse,
};
use dioxus::prelude::*;
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{send, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::platform::now_millis;

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
    let _ = send(&req);
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut entries: Signal<Vec<HistoryEntry>> = use_signal(Vec::new);
    let mut query: Signal<String> = use_signal(String::new);
    let mut offset: Signal<u32> = use_signal(|| 0);
    let mut has_more: Signal<bool> = use_signal(|| true);
    let mut request_id: Signal<u64> = use_signal(|| 0);
    let mut last_reset_id: Signal<u64> = use_signal(|| 0);

    let _listener = use_listener::<HistoryQueryResponse, _>(
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

    let _changed_listener = use_listener::<HistoryChangedEvent, _>(
        HISTORY_CHANGED_EVENT,
        move |_: HistoryChangedEvent| {
            let new_id = *request_id.peek() + 1;
            request_id.set(new_id);
            offset.set(0);
            last_reset_id.set(new_id);
            let q = query.peek().clone();
            emit_query(&q, 0, new_id);
        },
    );

    let load_more = move |e: Event<VisibleData>| {
        if !e.is_intersecting().unwrap_or(false) {
            return;
        }
        if !*has_more.read() || entries.read().is_empty() {
            return;
        }
        let new_offset = *offset.read() + 50;
        offset.set(new_offset);
        let new_id = *request_id.read() + 1;
        request_id.set(new_id);
        emit_query(&query.read(), new_offset, new_id);
    };

    let mut confirm_open: Signal<bool> = use_signal(|| false);

    let on_input = move |e: Event<FormData>| {
        query.set(e.value());
        let new_id = *request_id.read() + 1;
        request_id.set(new_id);
        offset.set(0);
        last_reset_id.set(new_id);
        emit_query(&query.read(), 0, new_id);
    };

    let groups = group_by_day(&entries.read(), now_millis());

    rsx! {
        div { class: "flex flex-col h-screen bg-background text-foreground",
            header { class: "p-3 border-b border-border flex gap-2 items-center",
                input {
                    class: "flex-1 bg-muted px-3 py-2 rounded text-sm outline-none",
                    placeholder: translate("history-search"),
                    value: "{query.read()}",
                    oninput: on_input,
                }
                button {
                    class: "px-3 py-2 text-xs bg-destructive text-destructive-foreground rounded",
                    onclick: move |_| confirm_open.set(true),
                    {translate("history-clear-all")}
                }
            }
            main { class: "flex-1 overflow-y-auto p-3 text-sm",
                for (label, group) in groups {
                    div { class: "text-xs text-muted-foreground uppercase mt-4 mb-1", "{label}" }
                    for entry in group {
                        div {
                            class: "flex items-center gap-2 py-1 border-b border-border hover:bg-foreground/[0.04] group cursor-pointer",
                            onclick: {
                                let url = entry.url.clone();
                                move |_| {
                                    let _ = send(&HistoryOpenRequest {
                                        url: url.clone(),
                                        in_new_stack: true,
                                    });
                                }
                            },
                            span { class: "text-xs text-muted-foreground w-12", "{format_time(entry.visit_created_at)}" }
                            Favicon {
                                favicon_url: entry.favicon_url.clone(),
                                url: entry.url.clone(),
                                class: "w-4 h-4 shrink-0 rounded-sm object-contain".to_string(),
                                globe_class: "w-4 h-4 shrink-0 text-muted-foreground".to_string(),
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
                                        let _ = send(&HistoryDeleteRequest { url_entity_bits: url_bits });
                                        entries.write().retain(|x| x.url_entity_bits != url_bits);
                                    }
                                },
                                "\u{00d7}"
                            }
                        }
                    }
                }
                div { class: "h-4", onvisible: load_more }
            }
        }
        if *confirm_open.read() {
            div { class: "fixed inset-0 bg-scrim-strong flex items-center justify-center z-50",
                div { class: "bg-card border border-border p-6 rounded max-w-sm",
                    h3 { class: "text-lg mb-2", {translate("history-clear-confirm")} }
                    p { class: "text-sm text-muted-foreground mb-4", {translate("history-clear-warning")} }
                    div { class: "flex gap-2 justify-end",
                        button {
                            class: "px-3 py-1 text-sm bg-muted rounded",
                            onclick: move |_| confirm_open.set(false),
                            {translate("history-cancel")}
                        }
                        button {
                            class: "px-3 py-1 text-sm bg-destructive text-destructive-foreground rounded",
                            onclick: move |_| {
                                let _ = send(&HistoryClearAllRequest);
                                entries.write().clear();
                                confirm_open.set(false);
                            },
                            {translate("history-clear-all")}
                        }
                    }
                }
            }
        }
    }
}

fn group_by_day(entries: &[HistoryEntry], now_ms: i64) -> Vec<(String, Vec<HistoryEntry>)> {
    let mut out: Vec<(String, Vec<HistoryEntry>)> = Vec::new();
    let mut current_day: Option<i64> = None;
    let now_day = now_ms / 86_400_000;
    for e in entries {
        let day = e.visit_created_at / 86_400_000;
        if current_day != Some(day) {
            let label = match now_day - day {
                0 => translate("history-today"),
                1 => translate("history-yesterday"),
                d if d < 7 => translate_with(
                    "history-days-ago",
                    &[("count", TranslationValue::Number(d))],
                ),
                _ => translate_with(
                    "history-day-offset",
                    &[("count", TranslationValue::Number(now_day - day))],
                ),
            };
            out.push((label, Vec::new()));
            current_day = Some(day);
        }
        out.last_mut().unwrap().1.push(e.clone());
    }
    out
}

fn format_time(ms: i64) -> String {
    let total_sec = ms / 1000;
    let h = (total_sec % 86400) / 3600;
    let m = (total_sec % 3600) / 60;
    format!("{:02}:{:02}", h, m)
}
