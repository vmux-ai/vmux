#![allow(non_snake_case)]

use crate::event::{
    HISTORY_CHANGED_EVENT, HISTORY_QUERY_RESPONSE_EVENT, HistoryChangedEvent,
    HistoryClearAllRequest, HistoryDeleteRequest, HistoryEntry, HistoryOpenRequest,
    HistoryQueryRequest, HistoryQueryResponse,
};
use dioxus::prelude::*;
use vmux_ui::components::alert_dialog::{
    AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
    AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
};
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

    let mut confirm_open = use_signal(|| Some(false));

    let on_input = move |e: Event<FormData>| {
        query.set(e.value());
        let new_id = *request_id.read() + 1;
        request_id.set(new_id);
        offset.set(0);
        last_reset_id.set(new_id);
        emit_query(&query.read(), 0, new_id);
    };

    let groups = group_by_day(&entries.read(), now_millis());
    let entry_count = entries.read().len();

    rsx! {
        div { class: "flex h-full min-h-0 flex-col bg-background text-foreground",
            header { class: "flex items-center justify-between border-b border-border px-5 py-4",
                h1 { class: "text-lg font-semibold tracking-tight", {translate("history-title")} }
                if entry_count > 0 {
                    span { class: "flex min-w-7 items-center justify-center rounded-full border border-border bg-card px-2.5 py-1 text-xs font-medium text-muted-foreground",
                        "{entry_count}"
                    }
                }
            }
            main { class: "min-h-0 flex-1 overflow-y-auto px-5 py-5 text-sm",
                div { class: "mx-auto w-full max-w-4xl",
                    div { class: "glass mb-6 flex items-center gap-2 rounded-xl border border-border/70 p-2",
                        svg { class: "ml-1 size-4 shrink-0 text-muted-foreground", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8",
                            circle { cx: "11", cy: "11", r: "7" }
                            path { d: "m20 20-3.5-3.5" }
                        }
                        input {
                            class: "min-w-0 flex-1 bg-transparent px-1 py-1.5 text-sm outline-none placeholder:text-muted-foreground",
                            placeholder: translate("history-search"),
                            value: "{query.read()}",
                            oninput: on_input,
                        }
                        button {
                            class: "rounded-lg px-2.5 py-1.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive disabled:pointer-events-none disabled:opacity-40",
                            disabled: entry_count == 0,
                            onclick: move |_| confirm_open.set(Some(true)),
                            {translate("history-clear-all")}
                        }
                    }
                    for (label, group) in groups {
                        section { class: "mb-6",
                            h2 { class: "mb-2 px-1 text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground", "{label}" }
                            div { class: "overflow-hidden rounded-xl border border-border bg-card/30",
                                for entry in group {
                                    div {
                                        class: "group flex cursor-pointer items-center gap-3 border-b border-border/70 px-3 py-2.5 transition-colors last:border-b-0 hover:bg-foreground/[0.04]",
                                        onclick: {
                                            let url = entry.url.clone();
                                            move |_| {
                                                let _ = send(&HistoryOpenRequest {
                                                    url: url.clone(),
                                                    in_new_stack: true,
                                                });
                                            }
                                        },
                                        div { class: "flex size-8 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.055]",
                                            Favicon {
                                                favicon_url: entry.favicon_url.clone(),
                                                url: entry.url.clone(),
                                                class: "size-4 shrink-0 rounded-sm object-contain".to_string(),
                                                globe_class: "size-4 shrink-0 text-muted-foreground".to_string(),
                                            }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col gap-0.5",
                                            span { class: "truncate text-sm font-medium text-foreground",
                                                if entry.title.is_empty() { "{entry.url}" } else { "{entry.title}" }
                                            }
                                            if !entry.title.is_empty() && entry.title != entry.url {
                                                span { class: "truncate text-xs text-muted-foreground", "{entry.url}" }
                                            }
                                        }
                                        span { class: "shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground/70", "{format_time(entry.visit_created_at)}" }
                                        button {
                                            class: "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-destructive/10 hover:text-destructive",
                                            aria_label: format!(
                                                "{}: {}",
                                                translate("common-remove"),
                                                if entry.title.is_empty() { entry.url.as_str() } else { entry.title.as_str() },
                                            ),
                                            onclick: {
                                                let url_bits = entry.url_entity_bits;
                                                move |e: Event<MouseData>| {
                                                    e.stop_propagation();
                                                    let _ = send(&HistoryDeleteRequest { url_entity_bits: url_bits });
                                                    entries.write().retain(|x| x.url_entity_bits != url_bits);
                                                }
                                            },
                                            svg { class: "size-3.5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8",
                                                path { d: "M18 6 6 18M6 6l12 12" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "h-4", onvisible: load_more }
                }
            }
        }
        AlertDialogRoot {
            open: Into::<ReadSignal<Option<bool>>>::into(confirm_open),
            on_open_change: Callback::new(move |open| confirm_open.set(Some(open))),
            default_open: false,
            attributes: vec![],
            AlertDialogContent { attributes: vec![],
                AlertDialogTitle { attributes: vec![], {translate("history-clear-confirm")} }
                AlertDialogDescription { attributes: vec![], {translate("history-clear-warning")} }
                AlertDialogActions { attributes: vec![],
                    AlertDialogCancel {
                        attributes: vec![],
                        on_click: Some(EventHandler::new(move |_| confirm_open.set(Some(false)))),
                        {translate("history-cancel")}
                    }
                    AlertDialogAction {
                        attributes: vec![],
                        on_click: Some(EventHandler::new(move |_| {
                            let _ = send(&HistoryClearAllRequest);
                            entries.write().clear();
                            confirm_open.set(Some(false));
                        })),
                        {translate("history-clear-all")}
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
