//! History list UI (grouped by day, search, open in active pane).

use crate::cef::{
    emit_clear_history, emit_open_in_pane, random_history_sync_nonce,
    request_history_sync_from_host, run_history_bridge_loop, try_install_cef_history_listener,
};
use crate::payload::HistoryEntryWire;

use dioxus::prelude::*;
use vmux_ui::dioxus_ext::{attributes, merge_attributes};
use vmux_ui::webview::components::{
    button::{Button, ButtonVariant},
    icon::{Icon, ViewBox},
    input::Input,
    label::Label,
    UiDivider, UiDividerVariant, UiInputShell, UiPanel, UiRow, UiStack, UiText, UiTextSize,
    UiTextTone,
};
use vmux_ui::webview::web_color;

use futures_channel::mpsc::unbounded;
use std::net::IpAddr;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::{EventTarget, window};

const MS_PER_DAY: i64 = 86400_000;

// Tailwind-only layout (see `assets/input.css` for base html/body only).
const TW_ROOT: &str = "flex h-full min-h-0 min-w-0 w-full flex-1 flex-col overflow-hidden bg-[linear-gradient(180deg,#16171c_0%,#0e0f12_48%,#0c0d10_100%)]";
const TW_HEADER: &str = "sticky top-0 z-20 shrink-0 border-b border-white/[0.06] bg-[linear-gradient(180deg,#16171c_0%,#15161b_72%,#141518_100%)] px-4 pb-3 pt-4";
const TW_CLEAR_BTN: &str = "shrink-0 cursor-pointer rounded-lg border border-red-400/35 bg-red-400/[0.08] px-[0.65rem] py-[0.35rem] text-[11px] font-medium text-red-300/95 transition-colors duration-150 hover:border-red-400/55 hover:bg-red-400/[0.14] hover:text-red-200 disabled:cursor-not-allowed disabled:border-white/[0.08] disabled:bg-white/[0.03] disabled:text-white/25 disabled:opacity-[0.35]";
const TW_SEARCH: &str = "w-full rounded-xl border border-white/[0.08] bg-white/[0.04] py-[0.65rem] pl-9 pr-3 text-[13px] text-white/90 shadow-[inset_0_1px_2px_rgba(0,0,0,0.2)] outline-none placeholder:text-white/35 focus:border-sky-400/35 focus:bg-white/[0.06]";
const TW_ROW_BTN: &str = "group m-0 flex w-full max-w-full min-w-0 cursor-pointer appearance-none flex-col items-stretch gap-[0.35rem] rounded-xl border border-white/[0.06] bg-white/[0.03] py-[0.65rem] px-3 text-left font-inherit text-inherit shadow-[0_1px_3px_rgba(0,0,0,0.12)] transition-colors duration-150 hover:border-sky-400/28 hover:bg-sky-400/[0.07]";
const TW_FAVICON: &str = "mt-px h-[18px] w-[18px] shrink-0 rounded object-contain border border-white/[0.08] bg-white/[0.06] transition-colors duration-150 group-hover:border-sky-400/25 group-hover:bg-sky-400/[0.08]";
const TW_LOAD_MORE: &str = "mt-1.5 block w-full cursor-pointer rounded-[10px] border border-white/12 bg-white/[0.05] py-2 px-3 text-center text-xs text-white/78 transition-colors duration-150 hover:border-sky-400/35 hover:bg-sky-400/10 hover:text-white/90";
const TW_STREAM_HINT: &str = "mt-2 text-center text-[10px] text-white/30";
const TW_RETRY_BTN: &str = "mt-3 cursor-pointer rounded-lg border border-amber-400/40 bg-amber-400/[0.12] px-3 py-1.5 text-[11px] font-medium text-amber-100/95 transition-colors duration-150 hover:border-amber-400/60 hover:bg-amber-400/20";

/// `id` shared by [`Label`] and the filter field ([`Input`] + [`UiInputShell`]).
const HISTORY_SEARCH_ID: &str = "history-search";

/// First paint shows this many rows; avoids mounting hundreds of DOM nodes at once (faster CEF composite).
const INITIAL_VISIBLE_ROWS: usize = 48;
const LOAD_MORE_ROWS: usize = 72;
const CEF_LISTENER_ATTEMPTS_MAX: u32 = 200;

fn day_label(now_ms: i64, entry_ms: i64) -> &'static str {
    let age = (now_ms - entry_ms).max(0) / MS_PER_DAY;
    if age < 1 {
        "Today"
    } else if age < 2 {
        "Yesterday"
    } else {
        "Older"
    }
}

/// Local date + time (medium date, short time), e.g. `Mar 30, 2026, 3:45 PM`.
fn format_visit_stamp(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let d = js_sys::Date::new(&JsValue::from_f64(ms as f64));
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &opts,
        &JsValue::from_str("dateStyle"),
        &JsValue::from_str("medium"),
    );
    let _ = js_sys::Reflect::set(
        &opts,
        &JsValue::from_str("timeStyle"),
        &JsValue::from_str("short"),
    );
    d.to_locale_string("en-US", &opts.into())
        .as_string()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "—".to_string())
}

fn visit_stamp_display(ms: i64) -> String {
    let s = format_visit_stamp(ms);
    if s.is_empty() {
        "No visit time recorded".to_string()
    } else {
        s
    }
}

fn visit_title_tooltip(ms: i64, url: &str) -> String {
    if ms <= 0 {
        return url.to_string();
    }
    let d = js_sys::Date::new(&JsValue::from_f64(ms as f64));
    let iso = d.to_iso_string();
    let iso = iso.as_string().unwrap_or_default();
    if iso.is_empty() {
        url.to_string()
    } else {
        format!("{iso} — {url}")
    }
}

fn row_tooltip(visit_ms: i64, url: &str, favicon_cached_at_ms: Option<i64>) -> String {
    let mut t = visit_title_tooltip(visit_ms, url);
    if let Some(fc) = favicon_cached_at_ms.filter(|&x| x > 0) {
        let d = js_sys::Date::new(&JsValue::from_f64(fc as f64));
        if let Some(iso) = d.to_iso_string().as_string() {
            if !iso.is_empty() {
                t.push_str("\nFavicon cached: ");
                t.push_str(&iso);
            }
        }
    }
    t
}

fn confirm_clear_history() -> bool {
    let Some(w) = window() else {
        return false;
    };
    w.confirm_with_message("Clear all history? This cannot be undone.")
        .unwrap_or(false)
}

/// Same host rules as `vmux_core::favicon_url_for_page_url` (this crate’s WASM build does not link `vmux_core`).
fn page_host_for_favicon_url(url: &str) -> Option<String> {
    let t = url.trim();
    if t.is_empty() || t.starts_with("data:") || t.starts_with("about:") {
        return None;
    }
    let rest = t
        .strip_prefix("https://")
        .or_else(|| t.strip_prefix("http://"))
        .unwrap_or("");
    let host_end = rest.find('/').unwrap_or(rest.len());
    let host = rest[..host_end].rsplit('@').next().unwrap_or("").trim();
    if host.is_empty() {
        return None;
    }
    let host = if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        inner
    } else {
        host
    };
    if host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    if host.parse::<IpAddr>().is_ok() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

fn favicon_url_for_page_url(url: &str) -> Option<String> {
    let host = page_host_for_favicon_url(url)?;
    Some(format!(
        "https://t3.gstatic.com/faviconV2?client=SOCIAL&type=FAVICON&fallback_opts=TYPE,SIZE,URL&url=http://{host}/&size=32"
    ))
}

fn url_display_parts(url: &str) -> (String, String) {
    let u = url.trim();
    if let Some(rest) = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"))
    {
        if let Some((host, path)) = rest.split_once('/') {
            (host.to_string(), format!("/{}", path))
        } else {
            (rest.to_string(), "/".to_string())
        }
    } else {
        (u.to_string(), String::new())
    }
}

/// Case-insensitive substring match for URLs without allocating a lowercased copy of `url`
/// (hot path when the filter is non-empty and the list is large). Non-ASCII needles fall back
/// to full Unicode lowercasing.
fn url_matches_filter(url: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if !needle_lower.is_ascii() {
        return url.to_lowercase().contains(needle_lower);
    }
    let n = needle_lower.as_bytes();
    let hay = url.as_bytes();
    if n.len() > hay.len() {
        return false;
    }
    hay.windows(n.len()).any(|w| {
        w.iter()
            .zip(n.iter())
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}

/// Filter + day grouping by index (no cloning of [`HistoryEntryWire`] — can be thousands of URLs).
fn filter_and_group_by_day_indices(
    now_ms: i64,
    entries: &[HistoryEntryWire],
    needle_lower: &str,
) -> Vec<(&'static str, Vec<usize>)> {
    let mut out: Vec<(&'static str, Vec<usize>)> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if !url_matches_filter(&entry.url, needle_lower) {
            continue;
        }
        let label = day_label(now_ms, entry.visited_at_ms);
        match out.last_mut() {
            Some((l, rows)) if *l == label => rows.push(i),
            _ => out.push((label, vec![i])),
        }
    }
    out
}

#[derive(Clone, PartialEq)]
struct HistoryRowModel {
    /// Stable key for list reconciliation (`url` + `visited_at` can repeat).
    row_key: String,
    url: String,
    stamp: String,
    title: String,
    host: String,
    path: String,
    favicon_url: Option<String>,
    favicon_cached_at_ms: Option<i64>,
}

fn build_row_model(entry: &HistoryEntryWire, stable_index: usize) -> HistoryRowModel {
    let (host, path) = url_display_parts(&entry.url);
    let favicon_url = entry
        .favicon_url
        .clone()
        .or_else(|| favicon_url_for_page_url(&entry.url));
    HistoryRowModel {
        row_key: format!("h{stable_index}"),
        url: entry.url.clone(),
        stamp: visit_stamp_display(entry.visited_at_ms),
        title: row_tooltip(entry.visited_at_ms, &entry.url, entry.favicon_cached_at_ms),
        host,
        path,
        favicon_url,
        favicon_cached_at_ms: entry.favicon_cached_at_ms,
    }
}

#[derive(Clone, PartialEq)]
struct PreparedRaw {
    grouped: Vec<(&'static str, Vec<usize>)>,
    total_rows: usize,
}

/// Single memo output: avoids two [`use_memo`] hooks both reading `entries`/`filter`, which can
/// panic in the Dioxus reactive layer (see comment on nested memos below).
#[derive(Clone, PartialEq)]
struct PreparedHistoryView {
    total_rows: usize,
    visible_grouped: Vec<(&'static str, Vec<HistoryRowModel>)>,
}

/// Filter + day grouping only (indices only). Row strings are built only for the visible slice.
fn prepare_history_raw(entries: &[HistoryEntryWire], filter_trimmed: &str) -> PreparedRaw {
    let now_ms = js_sys::Date::now() as i64;
    let q = filter_trimmed.to_lowercase();
    let grouped = filter_and_group_by_day_indices(now_ms, entries, &q);
    let total_rows: usize = grouped.iter().map(|(_, r)| r.len()).sum();
    PreparedRaw {
        grouped,
        total_rows,
    }
}

/// Nudges `vmux_request_history` until `bridge_sync_pending` clears or we mark stalled.
async fn run_history_resync_timeouts(
    nonce: u32,
    mut pending_sig: Signal<Option<u32>>,
    mut host_msg_sig: Signal<String>,
    mut host_snap_sig: Signal<bool>,
    mut stalled_sig: Signal<bool>,
) {
    for ms in [16u32, 48, 120] {
        gloo_timers::future::TimeoutFuture::new(ms).await;
        request_history_sync_from_host(Some(nonce));
    }
    for _ in 0..12 {
        if pending_sig.peek().is_none() {
            break;
        }
        gloo_timers::future::TimeoutFuture::new(200).await;
        if pending_sig.peek().is_none() {
            break;
        }
        request_history_sync_from_host(*pending_sig.peek());
    }
    if pending_sig.peek().is_some() {
        gloo_timers::future::TimeoutFuture::new(400).await;
    }
    if pending_sig.peek().is_some() {
        stalled_sig.set(true);
        host_snap_sig.set(true);
        pending_sig.set(None);
        host_msg_sig.set(
            "History sync is still starting (use Retry or focus another pane, then back)."
                .to_string(),
        );
    }
}

fn spawn_history_resync_timeouts(
    nonce: u32,
    pending_sig: Signal<Option<u32>>,
    host_msg_sig: Signal<String>,
    host_snap_sig: Signal<bool>,
    stalled_sig: Signal<bool>,
) {
    spawn(async move {
        run_history_resync_timeouts(nonce, pending_sig, host_msg_sig, host_snap_sig, stalled_sig)
            .await;
    });
}

/// User-initiated or window-focus retry after startup sync timed out (focus-only host invalidation
/// misses the case where the history pane was already active).
fn retry_history_sync_after_stall(
    mut bridge_sync_pending: Signal<Option<u32>>,
    mut host_snapshot_received: Signal<bool>,
    mut history_stream_complete: Signal<bool>,
    mut history_sync_stalled: Signal<bool>,
    mut host_progress_message: Signal<String>,
    mut host_progress_percent: Signal<u8>,
) {
    let nonce = random_history_sync_nonce();
    bridge_sync_pending.set(Some(nonce));
    history_sync_stalled.set(false);
    host_snapshot_received.set(false);
    history_stream_complete.set(true);
    host_progress_message.set("Fetching history...".to_string());
    host_progress_percent.set(65);
    request_history_sync_from_host(Some(nonce));
    spawn_history_resync_timeouts(
        nonce,
        bridge_sync_pending,
        host_progress_message,
        host_snapshot_received,
        history_sync_stalled,
    );
}

fn truncate_and_materialize(
    entries: &[HistoryEntryWire],
    grouped: &[(&'static str, Vec<usize>)],
    limit: usize,
) -> Vec<(&'static str, Vec<HistoryRowModel>)> {
    let mut out = Vec::new();
    let mut count = 0usize;
    for &(heading, ref row_indices) in grouped {
        if count >= limit {
            break;
        }
        let mut chunk: Vec<HistoryRowModel> = Vec::new();
        for &idx in row_indices {
            if count >= limit {
                break;
            }
            let Some(entry) = entries.get(idx) else {
                continue;
            };
            chunk.push(build_row_model(entry, count));
            count += 1;
        }
        if !chunk.is_empty() {
            out.push((heading, chunk));
        }
    }
    out
}

#[component]
fn HistoryRow(model: HistoryRowModel) -> Element {
    let HistoryRowModel {
        url,
        stamp,
        title,
        host,
        path,
        favicon_url,
        favicon_cached_at_ms: _,
        ..
    } = model;
    let open_url = url.clone();
    rsx! {
        Button {
            variant: ButtonVariant::Ghost,
            onclick: move |_| emit_open_in_pane(&open_url),
            attributes: merge_attributes(vec![attributes!(button {
                class: TW_ROW_BTN,
                role: "listitem",
                r#type: "button",
                title: "{title}",
            })]),
            div { class: "flex w-full min-w-0 items-start justify-between gap-x-3 gap-y-2",
                div { class: "flex min-w-0 flex-1 items-start gap-2",
                    if let Some(src) = favicon_url {
                        img {
                            class: "{TW_FAVICON}",
                            src: "{src}",
                            alt: "",
                            loading: "lazy",
                            decoding: "async",
                        }
                    } else {
                        div { class: "{TW_FAVICON}" }
                    }
                    div { class: "min-w-0 flex-1",
                        div { class: "truncate text-[13px] font-medium text-sky-300/95 group-hover:text-sky-200", "{host}" }
                        if !path.is_empty() {
                            div { class: "truncate font-mono text-[11px] text-white/45 group-hover:text-white/55", "{path}" }
                        }
                    }
                }
                div { class: "max-w-[42%] shrink-0 truncate text-right text-[10px] tabular-nums leading-snug text-white/48 whitespace-nowrap group-hover:text-white/62", "{stamp}" }
            }
            div { class: "min-w-0 w-full truncate font-mono text-[10px] leading-snug text-white/26 group-hover:text-white/34", "{url}" }
        }
    }
}

#[component]
pub fn App() -> Element {
    let mut entries = use_signal(Vec::<HistoryEntryWire>::new);
    let mut bridge_sync_pending = use_signal(|| None::<u32>);
    let mut cef_listener_ready = use_signal(|| false);
    let mut host_snapshot_received = use_signal(|| false);
    // `false` while the host is still appending older rows after the first IPC chunk.
    let mut history_stream_complete = use_signal(|| true);
    let mut filter = use_signal(String::new);
    let mut visible_limit = use_signal(|| INITIAL_VISIBLE_ROWS);
    let mut chrome_progress_percent = use_signal(|| 5u8);
    let mut chrome_progress_message = use_signal(|| "Waiting for CEF to start...".to_string());
    let mut host_progress_stage_sig = use_signal(|| "startup".to_string());
    let mut host_progress_message = use_signal(|| "Fetching history...".to_string());
    let mut host_progress_percent = use_signal(|| 12u8);
    let mut history_sync_stalled = use_signal(|| false);

    // `use_hook`: run once per mount. `use_effect` would resubscribe when captured signals change
    // and could orphan the CEF listener + channel (see `try_install_cef_history_listener`).
    use_hook(move || {
        let (tx, rx) = unbounded();
        spawn(async move {
            // Poll quickly: `cef` appears soon after navigation; 32ms×120 was ~4s worst-case
            // before we even asked the host for history.
            let mut rx = Some(rx);
            for attempt in 0..CEF_LISTENER_ATTEMPTS_MAX {
                let pct =
                    5u8.saturating_add((((attempt + 1) * 50) / CEF_LISTENER_ATTEMPTS_MAX) as u8);
                chrome_progress_percent.set(pct.min(55));
                chrome_progress_message.set("Waiting for CEF to start...".to_string());
                if try_install_cef_history_listener(tx.clone()) {
                    let nonce = random_history_sync_nonce();
                    bridge_sync_pending.set(Some(nonce));
                    cef_listener_ready.set(true);
                    chrome_progress_percent.set(60);
                    chrome_progress_message.set("CEF ready.".to_string());
                    host_progress_stage_sig.set("request".to_string());
                    host_progress_message.set("Fetching history...".to_string());
                    host_progress_percent.set(65);
                    history_sync_stalled.set(false);
                    request_history_sync_from_host(Some(nonce));
                    // Resync timers must not block `run_history_bridge_loop`: pending only clears when
                    // `rx` is drained; a prior sequential loop delayed the UI by up to ~2.4s.
                    spawn_history_resync_timeouts(
                        nonce,
                        bridge_sync_pending,
                        host_progress_message,
                        host_snapshot_received,
                        history_sync_stalled,
                    );
                    let Some(rx) = rx.take() else {
                        return;
                    };
                    run_history_bridge_loop(
                        rx,
                        entries,
                        bridge_sync_pending,
                        host_snapshot_received,
                        history_stream_complete,
                        host_progress_stage_sig,
                        host_progress_message,
                        host_progress_percent,
                    )
                    .await;
                    return;
                }
                // Tight early polling so we register `cef.listen` and emit `vmux_request_history`
                // soon after CEF injects `window.cef` (was 8ms×80 ≈ 640ms before first batch).
                let delay = if attempt < 60 {
                    0
                } else if attempt < 160 {
                    4
                } else {
                    20
                };
                gloo_timers::future::TimeoutFuture::new(delay).await;
            }
            cef_listener_ready.set(true);
            host_snapshot_received.set(true);
            if let Some(rx) = rx {
                run_history_bridge_loop(
                    rx,
                    entries,
                    bridge_sync_pending,
                    host_snapshot_received,
                    history_stream_complete,
                    host_progress_stage_sig,
                    host_progress_message,
                    host_progress_percent,
                )
                .await;
            }
        });
    });

    use_hook(move || {
        let Some(w) = window() else {
            return;
        };
        let stalled = history_sync_stalled;
        let entries_sig = entries;
        let bridge_sync_pending = bridge_sync_pending;
        let host_snapshot_received = host_snapshot_received;
        let history_stream_complete = history_stream_complete;
        let host_progress_message = host_progress_message;
        let host_progress_percent = host_progress_percent;
        let closure = Closure::wrap(Box::new(move |_ev: JsValue| {
            if stalled() && entries_sig().is_empty() {
                retry_history_sync_after_stall(
                    bridge_sync_pending,
                    host_snapshot_received,
                    history_stream_complete,
                    stalled,
                    host_progress_message,
                    host_progress_percent,
                );
            }
        }) as Box<dyn FnMut(JsValue)>);
        let target: &EventTarget = w.as_ref();
        let _ = target.add_event_listener_with_callback("focus", closure.as_ref().unchecked_ref());
        closure.forget();
    });

    use_effect(move || {
        let _ = filter();
        visible_limit.set(INITIAL_VISIBLE_ROWS);
    });

    let prepared = use_memo(move || {
        let list = entries();
        let p = prepare_history_raw(&list, filter().trim());
        let visible_grouped = truncate_and_materialize(&list, &p.grouped, visible_limit());
        PreparedHistoryView {
            total_rows: p.total_rows,
            visible_grouped,
        }
    });

    let prepared_inner = prepared.read();
    let total_rows = prepared_inner.total_rows;
    let grouped_for_view = prepared_inner.visible_grouped.clone();
    let limit = visible_limit();
    let has_more = total_rows > limit;
    let next_batch = LOAD_MORE_ROWS.min(total_rows.saturating_sub(limit));
    let filter_trimmed = filter().trim().to_string();
    let chrome_loading = !cef_listener_ready();
    let list_loading = cef_listener_ready() && !host_snapshot_received();
    let chrome_progress_width = format!("width: {}%;", chrome_progress_percent());
    let host_progress_width = format!("width: {}%;", host_progress_percent());
    let host_msg = host_progress_message();
    let chrome_agentic = chrome_progress_message();
    let host_agentic = host_msg;

    rsx! {
        div {
            id: "root",
            class: "{TW_ROOT}",
            if chrome_loading {
                div {
                    class: "flex min-h-0 min-w-0 flex-1 flex-col items-center justify-center px-4 py-10",
                    UiPanel {
                        aria_busy: Some(true),
                        aria_label: Some("Starting history UI".to_string()),
                        UiStack {
                            class: "items-center gap-3",
                            UiText {
                                tone: UiTextTone::Muted,
                                size: UiTextSize::Sm,
                                "Starting…"
                            }
                            div {
                                class: "{web_color::LOADING_TRACK}",
                                div { class: "{web_color::LOADING_PULSE}", style: "{chrome_progress_width}" }
                            }
                            span { class: "{web_color::SHIMMER_TEXT}", "{chrome_agentic}" }
                            UiText {
                                class: "tabular-nums text-white/20",
                                tone: UiTextTone::Inherit,
                                size: UiTextSize::Xxs,
                                "{chrome_progress_percent()}%"
                            }
                        }
                    }
                }
            } else {
                div {
                    class: "flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden",
                    header {
                        class: "{TW_HEADER} flex flex-col gap-3",
                        div { class: "flex flex-wrap items-start justify-between gap-3",
                            div { class: "min-w-0 flex-[1_1_12rem]",
                                h1 { class: "m-0 text-[13px] font-semibold tracking-[-0.02em] text-white/95", "History" }
                                p { class: "mb-0 mt-0.5 text-[11px] text-white/38", "Recent visits · click to open in this pane" }
                            }
                            {
                                let clear_disabled = entries().is_empty() || list_loading;
                                let mut clear_attrs = vec![attributes!(button {
                                    class: TW_CLEAR_BTN,
                                    r#type: "button",
                                })];
                                if clear_disabled {
                                    clear_attrs.push(attributes!(button { disabled: "true" }));
                                }
                                rsx! {
                                    Button {
                                        variant: ButtonVariant::Destructive,
                                        onclick: move |_| {
                                            if !confirm_clear_history() {
                                                return;
                                            }
                                            emit_clear_history();
                                            entries.set(Vec::new());
                                            history_stream_complete.set(true);
                                        },
                                        attributes: merge_attributes(clear_attrs),
                                        "Clear history"
                                    }
                                }
                            }
                        }
                        Label {
                            html_for: HISTORY_SEARCH_ID,
                            class: "sr-only",
                            "Filter history by URL"
                        }
                        UiInputShell {
                            leading: rsx! {
                                Icon {
                                    view_box: ViewBox::new(0, 0, 24, 24),
                                    stroke_width: 2.,
                                    class: "h-[14px] w-[14px]",
                                    circle { cx: 11, cy: 11, r: 8 }
                                    path { d: "m21 21-4.3-4.3" }
                                }
                            },
                            input: rsx! {
                                Input {
                                    oninput: move |ev: FormEvent| {
                                        filter.set(ev.value());
                                    },
                                    attributes: merge_attributes(vec![attributes!(input {
                                        id: HISTORY_SEARCH_ID,
                                        class: TW_SEARCH,
                                        r#type: "text",
                                        placeholder: "Filter by URL…",
                                        value: filter,
                                    })]),
                                    children: rsx! {},
                                }
                            },
                        }
                    }
                    div {
                        id: "hm-list",
                        class: "min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto px-3 pb-5 pt-1",
                        role: "list",
                        if list_loading {
                            UiPanel {
                                aria_busy: Some(true),
                                aria_label: Some("Loading history".to_string()),
                                UiStack {
                                    class: "items-center gap-3",
                                    UiText {
                                        tone: UiTextTone::Muted,
                                        size: UiTextSize::Sm,
                                        "Loading visits…"
                                    }
                                    div {
                                        class: "{web_color::LOADING_TRACK}",
                                        div { class: "{web_color::LOADING_PULSE}", style: "{host_progress_width}" }
                                    }
                                    span { class: "{web_color::SHIMMER_TEXT}", "{host_agentic}" }
                                    UiText {
                                        class: "tabular-nums text-white/20",
                                        tone: UiTextTone::Inherit,
                                        size: UiTextSize::Xxs,
                                        "{host_progress_percent()}%"
                                    }
                                }
                            }
                        } else if grouped_for_view.is_empty() {
                            if filter_trimmed.is_empty() {
                                if history_sync_stalled() {
                                    UiPanel {
                                        replace_default: true,
                                        class: Some("flex flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-amber-300/20 bg-amber-300/[0.04] px-6 py-14 text-center".to_string()),
                                        UiStack {
                                            class: "items-center gap-2",
                                            UiText {
                                                class: "text-[13px] text-amber-100/90",
                                                tone: UiTextTone::Inherit,
                                                size: UiTextSize::Inherit,
                                                "Still waiting for history engine."
                                            }
                                            UiText {
                                                class: "text-[11px] text-amber-100/55",
                                                tone: UiTextTone::Inherit,
                                                size: UiTextSize::Inherit,
                                                "Click Retry, switch away and back to this pane, or refocus the window."
                                            }
                                            Button {
                                                variant: ButtonVariant::Outline,
                                                onclick: move |_| {
                                                    retry_history_sync_after_stall(
                                                        bridge_sync_pending,
                                                        host_snapshot_received,
                                                        history_stream_complete,
                                                        history_sync_stalled,
                                                        host_progress_message,
                                                        host_progress_percent,
                                                    );
                                                },
                                                attributes: merge_attributes(vec![attributes!(button {
                                                    class: TW_RETRY_BTN,
                                                    r#type: "button",
                                                })]),
                                                "Retry sync"
                                            }
                                        }
                                    }
                                } else {
                                    UiPanel {
                                        replace_default: true,
                                        class: Some("flex flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-white/10 bg-white/[0.02] px-6 py-14 text-center".to_string()),
                                        UiStack {
                                            class: "items-center gap-2",
                                            UiText {
                                                class: "text-[13px] text-white/50",
                                                tone: UiTextTone::Inherit,
                                                size: UiTextSize::Inherit,
                                                "No history yet."
                                            }
                                            UiText {
                                                class: "text-[11px] text-white/28",
                                                tone: UiTextTone::Inherit,
                                                size: UiTextSize::Inherit,
                                                "Browse in another pane to build history."
                                            }
                                        }
                                    }
                                }
                            } else {
                                UiPanel {
                                    replace_default: true,
                                    class: Some("flex flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-white/10 bg-white/[0.02] px-6 py-14 text-center".to_string()),
                                    UiStack {
                                        class: "items-center gap-2",
                                        UiText {
                                            class: "text-[13px] text-white/50",
                                            tone: UiTextTone::Inherit,
                                            size: UiTextSize::Inherit,
                                            "No entries match your filter."
                                        }
                                        UiText {
                                            class: "text-[11px] text-white/28",
                                            tone: UiTextTone::Inherit,
                                            size: UiTextSize::Inherit,
                                            "Try a shorter or different URL fragment."
                                        }
                                    }
                                }
                            }
                        } else {
                            for (gi, (heading, rows)) in grouped_for_view.iter().cloned().enumerate() {
                                section {
                                    key: "g{gi}",
                                    class: "mb-[1.1rem] min-w-0 last:mb-0",
                                    UiRow {
                                        class: "sticky top-0 z-10 mb-1.5 items-center gap-2 bg-[linear-gradient(180deg,rgba(14,15,18,0.97)_60%,transparent)] px-1 pb-0 pt-2",
                                        UiText {
                                            class: "text-[10px] font-semibold uppercase tracking-[0.14em] text-white/35",
                                            tone: UiTextTone::Inherit,
                                            size: UiTextSize::Inherit,
                                            "{heading}"
                                        }
                                        UiDivider { variant: UiDividerVariant::HorizontalFade }
                                    }
                                    div { class: "flex min-w-0 flex-col gap-2",
                                        for model in rows {
                                            HistoryRow { key: "{model.row_key}", model }
                                        }
                                    }
                                }
                            }
                            if has_more {
                                Button {
                                    variant: ButtonVariant::Outline,
                                    onclick: move |_| {
                                        visible_limit
                                            .set((visible_limit() + LOAD_MORE_ROWS).min(total_rows));
                                    },
                                    attributes: merge_attributes(vec![attributes!(button {
                                        class: TW_LOAD_MORE,
                                        r#type: "button",
                                    })]),
                                    "Show more ({next_batch})"
                                }
                            }
                            if !history_stream_complete() && !entries().is_empty() {
                                div { class: "{TW_STREAM_HINT}", "Loading older visits…" }
                            }
                        }
                    }
                }
            }
        }
    }
}
