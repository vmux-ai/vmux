//! Tiled history pane: [`Pane`] + [`Webview`](vmux_layout::Webview) + [`History`](vmux_layout::History), hotkeys, host IPC.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use leafwing_input_manager::prelude::ActionState;
use serde::Serialize;
use vmux_command::AppCommandRequestQueue;
use vmux_core::{
    NavigationHistory, NavigationHistoryEntry, NavigationHistoryPath, NavigationHistorySaveQueue,
    SessionSavePath, SessionSaveQueue, WebviewDocumentUrlEmit,
};
use vmux_input::{AppInputRoot, KeyAction, sync_cef_osr_focus_with_active_pane};
use vmux_layout::{
    Active, History, HistoryPaneNeedsUrl, HistoryPaneOpenedAt, HistoryPaneStandby, LayoutAxis,
    Layout, LoadingBarMaterial, Pane, PaneChromeLoadingBar, PaneChromeOwner, PaneChromeStrip,
    PaneLastUrl, SessionLayoutSnapshot,
    Webview, spawn_history_pane,
    try_split_active_history_existing_pane, try_split_active_history_pane,
};
use vmux_server::{DioxusUiWarmupSet, dioxus_embedded_warmup_system};
use vmux_settings::VmuxAppSettings;

use super::{UiPlugin, register_ui_plugin_dioxus_warmup};
use bevy_cef::prelude::{
    HostEmitEvent, Receive, RequestNavigate, WebviewExtendStandardMaterial, WebviewSource,
};
use bevy_cef_core::prelude::Browsers;

/// Loopback or env base URL for the history UI bundle.
#[derive(Resource, Default, Clone)]
pub struct HistoryUiBaseUrl(pub Option<String>);

#[derive(Resource, Default)]
pub struct HistoryUiUrlReceiver(pub Option<crossbeam_channel::Receiver<String>>);

#[derive(Resource, Default)]
pub struct HistoryUiChromeUnavailable(pub bool);

const HISTORY_UI_EMBEDDED_WAIT_SECS: f32 = 5.0;

/// Newest-first slice size for the first `vmux_history` emit (fast first paint).
const HISTORY_STREAM_FIRST_LEN: usize = 120;
/// Subsequent chunks, still ordered newest → older (same as [`NavigationHistory::entries`]).
const HISTORY_STREAM_CHUNK_LEN: usize = 320;

/// If `vmux_request_history` never reaches the host (strict JSON, dropped IPC), still push history
/// after the browser has a main frame and the pane has been open this long.
const WASM_LISTENER_FALLBACK: Duration = Duration::from_secs(5);

/// When the host emits before WASM has called `cef.listen`, the render process drops the payload
/// (`handle_listen_message` has no callback). Throttle retries so we do not spam IPC every frame.
const HISTORY_FALLBACK_EMIT_MIN_INTERVAL: Duration = Duration::from_millis(400);

/// `wasm_sync_webviews` entry **or** fallback when the listener announce was lost / pane recreated.
fn history_pane_listener_ready(
    state: &HistoryUiEmitState,
    e: Entity,
    opened: &Query<&HistoryPaneOpenedAt>,
    browsers: &Browsers,
) -> bool {
    if state.wasm_sync_webviews.contains(&e) {
        return true;
    }
    if !browsers.host_emit_ready(&e) {
        return false;
    }
    let Ok(t0) = opened.get(e) else {
        return false;
    };
    t0.0.elapsed() >= WASM_LISTENER_FALLBACK
}

/// Only advance host dedupe (`last_revision`) after every target has announced `vmux_request_history`.
/// Otherwise a time-based fallback emit can run before `cef.listen` exists; the renderer drops the
/// payload while we still mark the revision as sent, wedging the WASM UI.
#[inline]
fn all_history_targets_wasm_announced(state: &HistoryUiEmitState, targets: &[Entity]) -> bool {
    targets.iter().all(|e| state.wasm_sync_webviews.contains(e))
}

#[inline]
fn history_payload_skip_append_false(a: &bool) -> bool {
    !*a
}

#[inline]
fn history_payload_skip_stream_done_true(d: &bool) -> bool {
    *d
}

const HISTORY_CHROME_UNAVAILABLE_HTML: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width"/><style>html,body{margin:0;background:#1a1a1a;color:#9aa0a6;font:12px system-ui,-apple-system,sans-serif;height:100%;}body{display:flex;align-items:center;justify-content:center;text-align:center;padding:8px 12px;}p{margin:0;line-height:1.4;}small{display:block;margin-top:6px;opacity:.75;font-size:11px;}</style></head><body><div><p>History UI did not load.</p><small>Set <code style="color:#bdc1c6">VMUX_HISTORY_UI_URL</code> or rebuild with <code style="color:#bdc1c6">crates/vmux_history/dist/</code> present.</small></div></body></html>"#;

/// `data:text/html;charset=utf-8,…` so the history pane does not use `cef://` inline URLs (see `spawn_history_pane`).
fn data_url_utf8_html(html: &str) -> String {
    use std::fmt::Write;
    const PREFIX: &str = "data:text/html;charset=utf-8,";
    let mut out = String::with_capacity(PREFIX.len() + html.len() * 3);
    out.push_str(PREFIX);
    for b in html.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => {
                let _ = write!(&mut out, "%{b:02X}");
            }
        }
    }
    out
}

/// In-flight newest→older stream for one [`NavigationHistory::revision`] and pane set.
#[derive(Clone, PartialEq, Eq)]
struct HistoryStreamInFlight {
    revision: u64,
    /// Next index into [`NavigationHistory::entries`] (0 = newest).
    next_offset: usize,
    targets: Vec<Entity>,
}

/// One-shot perf logs for history pane load (stall before CEF main frame, first host emit).
#[derive(Resource, Default)]
struct HistoryPanePerfLog {
    stall_warned: HashSet<Entity>,
    first_host_emit_logged: HashSet<Entity>,
}

/// Resource driving host→history WASM payload dedupe and streaming; required as the first argument to
/// [`apply_open_history_pane`] when calling it outside this plugin.
#[derive(Resource, Default)]
pub struct HistoryUiEmitState {
    last_revision: Option<u64>,
    /// When history panes are created after the last navigation, revision is unchanged; re-emit so new webviews receive the payload.
    last_target_entities: Vec<Entity>,
    /// Per-webview nonce from `vmux_history_sync_nonce`; echoed on the next `vmux_history` emit only for that pane.
    pending_history_sync_nonce: HashMap<Entity, u32>,
    /// History webviews that have emitted `vmux_request_history` (listener ready). Missing entries
    /// are covered by [`WASM_LISTENER_FALLBACK`] once CEF reports `host_emit_ready` for that pane.
    wasm_sync_webviews: HashSet<Entity>,
    /// Multi-frame IPC: first chunk is [`HISTORY_STREAM_FIRST_LEN`], then [`HISTORY_STREAM_CHUNK_LEN`].
    stream: Option<HistoryStreamInFlight>,
    /// Last time we attempted a host emit while still waiting for `vmux_request_history` (WASM sync).
    last_fallback_emit_at: Option<std::time::Instant>,
}

#[derive(Clone, Serialize)]
struct HistoryWireEntry {
    url: String,
    visited_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    favicon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    favicon_cached_at_ms: Option<i64>,
}

#[derive(Serialize)]
struct HistoryHostPayload {
    entries: Vec<HistoryWireEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync_nonce: Option<u32>,
    /// Continuation chunks after the first slice (`false` omitted for the first chunk).
    #[serde(skip_serializing_if = "history_payload_skip_append_false")]
    history_stream_append: bool,
    /// `false` while more chunks will follow for this sync (omitted when `true` for smaller JSON).
    #[serde(skip_serializing_if = "history_payload_skip_stream_done_true")]
    history_stream_done: bool,
}

#[derive(Serialize)]
struct HistoryProgressPayload {
    stage: &'static str,
    message: String,
    percent: u8,
}

#[derive(SystemParam)]
struct HistoryStandbyAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<WebviewExtendStandardMaterial>>,
    loading_bar_materials: ResMut<'w, Assets<LoadingBarMaterial>>,
}

/// Spawn one hidden/off-layout history pane early so first Cmd+Y can attach an already-running webview.
fn spawn_history_pane_standby(
    mut commands: Commands,
    mut assets: HistoryStandbyAssets,
    ready: Res<HistoryUiBaseUrl>,
    unavailable: Res<HistoryUiChromeUnavailable>,
    panes: Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    standby: Query<Entity, (With<History>, With<Pane>, With<Webview>, With<HistoryPaneStandby>)>,
) {
    if std::env::var("VMUX_HISTORY_DISABLE_PANE_STANDBY")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return;
    }
    if unavailable.0 || !standby.is_empty() || !panes.is_empty() {
        return;
    }
    let Some(url) = ready.0.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    let pane = spawn_history_pane(
        &mut commands,
        &mut assets.meshes,
        &mut assets.materials,
        &mut assets.loading_bar_materials,
        false,
        Some(url),
    );
    commands.entity(pane).insert((
        HistoryPaneStandby,
        // Keep off-layout standby effectively invisible but alive/running.
        Transform {
            translation: Vec3::new(0.0, 0.0, -1000.0),
            scale: Vec3::splat(1.0e-4),
            ..default()
        },
    ));
}

fn poll_history_url(mut ready: ResMut<HistoryUiBaseUrl>, rx: ResMut<HistoryUiUrlReceiver>) {
    if ready.0.is_some() {
        return;
    }
    let Some(ref r) = rx.0 else {
        return;
    };
    if let Ok(u) = r.try_recv() {
        ready.0 = Some(u);
    }
}

fn timeout_history_embedded(
    time: Res<Time>,
    mut wait_started: Local<Option<f32>>,
    ready: Res<HistoryUiBaseUrl>,
    mut unavailable: ResMut<HistoryUiChromeUnavailable>,
    rx: Res<HistoryUiUrlReceiver>,
) {
    if unavailable.0 || ready.0.is_some() {
        *wait_started = None;
        return;
    }
    if rx.0.is_none() {
        *wait_started = None;
        return;
    }
    let now = time.elapsed_secs();
    let start = wait_started.get_or_insert(now);
    if now - *start >= HISTORY_UI_EMBEDDED_WAIT_SECS {
        bevy::log::warn!(
            "vmux history: embedded HTTP server did not report a URL within {}s; using inline fallback",
            HISTORY_UI_EMBEDDED_WAIT_SECS
        );
        unavailable.0 = true;
        *wait_started = None;
    }
}

/// CEF warmup for the history Dioxus bundle when the loopback URL is ready (see `vmux_history::HistoryServerPlugin`).
pub fn history_dioxus_warmup_should_spawn(world: &mut World) -> Option<String> {
    let standby_enabled = !std::env::var("VMUX_HISTORY_DISABLE_PANE_STANDBY")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if standby_enabled {
        let mut q = world.query_filtered::<Entity, (With<Pane>, With<Webview>, With<History>, Without<HistoryPaneStandby>)>();
        if q.iter(world).next().is_none() {
            return None;
        }
    }
    if std::env::var("VMUX_HISTORY_DISABLE_CEF_WARMUP")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return None;
    }
    let chrome = world.get_resource::<HistoryUiChromeUnavailable>()?;
    if chrome.0 {
        return None;
    }
    let ready = world.get_resource::<HistoryUiBaseUrl>()?;
    let url = ready.0.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
    Some(url.to_string())
}

fn apply_history_url_to_panes(
    ready: Res<HistoryUiBaseUrl>,
    unavailable: Res<HistoryUiChromeUnavailable>,
    mut commands: Commands,
    mut state: ResMut<HistoryUiEmitState>,
    mut q: Query<(Entity, &mut WebviewSource), With<HistoryPaneNeedsUrl>>,
) {
    if unavailable.0 {
        let url = data_url_utf8_html(HISTORY_CHROME_UNAVAILABLE_HTML);
        let mut any = false;
        for (e, mut src) in &mut q {
            *src = WebviewSource::new(url.clone());
            commands.entity(e).remove::<HistoryPaneNeedsUrl>();
            any = true;
        }
        if any {
            state.last_revision = None;
            state.stream = None;
            state.wasm_sync_webviews.clear();
        }
        return;
    }
    let Some(url) = ready.0.as_ref() else {
        return;
    };
    let mut any = false;
    for (e, mut src) in &mut q {
        *src = WebviewSource::new(url.clone());
        commands.entity(e).remove::<HistoryPaneNeedsUrl>();
        any = true;
    }
    if any {
        // Real URL applied after `about:blank`: allow [`emit_history_to_panes`] to push again once
        // the WASM page has installed `cef.listen` (host emit may have been dropped earlier).
        state.last_revision = None;
        state.stream = None;
        state.wasm_sync_webviews.clear();
    }
}

/// Host emit can fire before the WASM page calls `cef.listen`, so the first payload is dropped.
/// The UI emits `vmux_request_history` once the listener is installed; we clear dedupe state and resend.
///
/// Include **standby** history webviews: the Dioxus app boots off-layout first; rejecting the first
/// `vmux_request_history` here left `wasm_sync_webviews` empty after promotion into a split.
fn on_vmux_request_history(
    trigger: On<Receive<WebviewDocumentUrlEmit>>,
    history: Query<(), (With<Pane>, With<Webview>, With<History>)>,
    mut state: ResMut<HistoryUiEmitState>,
) {
    let ev = trigger.event();
    if !ev.vmux_request_history {
        return;
    }
    if !history.contains(ev.webview) {
        return;
    }
    state.last_revision = None;
    state.stream = None;
    state.wasm_sync_webviews.insert(ev.webview);
    if let Some(n) = ev.vmux_history_sync_nonce {
        state.pending_history_sync_nonce.insert(ev.webview, n);
    }
}

/// Switching focus onto the history pane should refresh the list (same race as initial load).
/// Until **each** layout history webview has announced the listener (or the fallback timer elapses),
/// clear [`HistoryUiEmitState::last_revision`] **every** [`PostUpdate`] so [`emit_history_to_panes`]
/// cannot get stuck in the dedupe skip.
///
/// Only **non-standby** panes are counted so an off-layout standby browser cannot block the visible
/// history pane forever.
/// After restart or navigation, CEF may report [`Browsers::host_emit_ready`] only once OSR focus has
/// visited the history browser (`sync_osr_focus_to_active_pane`). If we already advanced dedupe
/// (`last_revision`) while `main_frame` was still missing, or skipped emit, the UI stays empty until
/// something else clears dedupe (e.g. clicking the pane). Clear when readiness **transitions** so the
/// next [`emit_history_to_panes`] pass can resend.
fn nudge_history_emit_when_history_host_emit_ready_rises(
    mut state: ResMut<HistoryUiEmitState>,
    history: Query<
        Entity,
        (
            With<Pane>,
            With<Webview>,
            With<History>,
            Without<HistoryPaneStandby>,
        ),
    >,
    browsers: NonSend<Browsers>,
    mut prev: Local<HashMap<Entity, bool>>,
) {
    let id_set: HashSet<Entity> = history.iter().collect();
    prev.retain(|e, _| id_set.contains(e));
    for e in history.iter() {
        let now = browsers.host_emit_ready(&e);
        let was = prev.get(&e).copied().unwrap_or(false);
        if now && !was {
            state.last_revision = None;
            state.stream = None;
        }
        prev.insert(e, now);
    }
}

fn nudge_history_emit_for_osr_wasm_timers(
    mut state: ResMut<HistoryUiEmitState>,
    history: Query<
        Entity,
        (
            With<Pane>,
            With<Webview>,
            With<History>,
            Without<HistoryPaneStandby>,
        ),
    >,
    opened: Query<&HistoryPaneOpenedAt>,
    browsers: NonSend<Browsers>,
) {
    let mut ids: Vec<Entity> = history.iter().collect();
    if ids.is_empty() {
        state.wasm_sync_webviews.clear();
        return;
    }
    let id_set: HashSet<Entity> = ids.iter().copied().collect();
    state.wasm_sync_webviews.retain(|e| id_set.contains(e));
    ids.sort_unstable();
    if ids
        .iter()
        .all(|&e| history_pane_listener_ready(&state, e, &opened, &browsers))
    {
        return;
    }
    state.last_revision = None;
    state.stream = None;
}

/// Re-push when [`NavigationHistory::revision`] changes or the set of on-layout history webviews changes
/// (new split, promotion from standby, session restore).
fn history_emit_reset_on_navigation_or_targets_changed(
    mut state: ResMut<HistoryUiEmitState>,
    hist: Res<NavigationHistory>,
    targets: Query<
        Entity,
        (
            With<Pane>,
            With<Webview>,
            With<History>,
            Without<HistoryPaneStandby>,
        ),
    >,
    mut prev: Local<Option<(u64, Vec<Entity>)>>,
) {
    let rev = hist.revision;
    let mut cur: Vec<Entity> = targets.iter().collect();
    cur.sort_unstable();
    let snapshot = (rev, cur);
    if prev.as_ref() != Some(&snapshot) {
        state.last_revision = None;
        state.stream = None;
        *prev = Some(snapshot);
    }
}

fn invalidate_history_emit_when_focusing_history_pane(
    active: Query<Entity, (With<Pane>, With<Active>)>,
    history: Query<
        (),
        (
            With<Pane>,
            With<Webview>,
            With<History>,
            Without<HistoryPaneStandby>,
        ),
    >,
    mut state: ResMut<HistoryUiEmitState>,
    mut prev_active: Local<Option<Entity>>,
) {
    let Ok(cur) = active.single() else {
        return;
    };
    let was_history = prev_active.as_ref().is_some_and(|&p| history.contains(p));
    let now_history = history.contains(cur);
    // Re-entering the same history pane (already active) does not flip `was_history`; host dedupe
    // would still skip a resend while WASM is stuck. Clear when focus *lands* on history so any
    // transition into this pane (including first focus) can emit.
    if now_history && (!was_history || prev_active.as_ref() != Some(&cur)) {
        state.last_revision = None;
        state.stream = None;
    }
    *prev_active = Some(cur);
}

fn history_wire_slice(
    entries: &[NavigationHistoryEntry],
    start: usize,
    take: usize,
) -> Vec<HistoryWireEntry> {
    entries
        .iter()
        .skip(start)
        .take(take)
        .map(|e| HistoryWireEntry {
            url: e.url.clone(),
            visited_at_ms: e.visited_at_ms,
            favicon_url: None,
            favicon_cached_at_ms: None,
        })
        .collect()
}

fn trigger_vmux_history_emit(
    commands: &mut Commands,
    perf: &mut HistoryPanePerfLog,
    opened: &Query<&HistoryPaneOpenedAt>,
    wv: Entity,
    payload: &HistoryHostPayload,
) {
    if perf.first_host_emit_logged.insert(wv) {
        if let Ok(t0) = opened.get(wv) {
            bevy::log::info!(
                "vmux history: first vmux_history host emit to pane {:?} after {:?} (CEF main frame ready; WASM UI may still be binding cef.listen)",
                wv,
                t0.0.elapsed()
            );
        }
    }
    commands.trigger(HostEmitEvent::new(wv, "vmux_history", payload));
}

fn trigger_vmux_history_progress_emit(
    commands: &mut Commands,
    wv: Entity,
    stage: &'static str,
    message: impl Into<String>,
    percent: u8,
) {
    let payload = HistoryProgressPayload {
        stage,
        message: message.into(),
        percent: percent.min(100),
    };
    commands.trigger(HostEmitEvent::new(wv, "vmux_history_progress", &payload));
}

fn emit_history_to_panes(
    mut commands: Commands,
    mut state: ResMut<HistoryUiEmitState>,
    mut perf: ResMut<HistoryPanePerfLog>,
    hist: Res<NavigationHistory>,
    targets: Query<
        Entity,
        (
            With<Pane>,
            With<Webview>,
            With<History>,
            Without<HistoryPaneStandby>,
        ),
    >,
    any_history: Query<Entity, With<History>>,
    opened: Query<&HistoryPaneOpenedAt>,
    browsers: NonSend<Browsers>,
) {
    let mut target_list: Vec<Entity> = targets.iter().collect();
    target_list.sort_unstable();

    if target_list.is_empty() {
        state.last_target_entities.clear();
        state.stream = None;
        perf.stall_warned.clear();
        perf.first_host_emit_logged.clear();
        // Standby (off-layout) history webviews still run WASM and send `vmux_request_history`.
        // Do not advance dedupe or wipe wasm sync until no [`History`] webviews exist — otherwise
        // promotion into a split never sees a matching `wasm_sync_webviews` entry.
        if any_history.is_empty() {
            state.last_revision = Some(hist.revision);
            state.wasm_sync_webviews.clear();
        }
        return;
    }

    if state.last_target_entities != target_list {
        let new_set: HashSet<Entity> = target_list.iter().copied().collect();
        state.wasm_sync_webviews.retain(|e| new_set.contains(e));
    }

    if let Some(st) = &state.stream {
        if st.revision != hist.revision || st.targets != target_list {
            state.stream = None;
        }
    }

    let targets_unchanged = state.last_target_entities == target_list;
    let revision_unchanged = state.last_revision == Some(hist.revision);
    let all_wasm = all_history_targets_wasm_announced(&state, &target_list);

    // Avoid HostEmit while CEF has not created a browser yet, or after teardown has begun (e.g.
    // kill-pane): `main_frame` / process messages can fault if we race despawn.
    //
    // [`Browsers::emit_event`] no-ops without a main frame but would still leave us thinking we
    // pushed history — do not advance dedupe until the frame exists so the next revision retries.
    if !target_list
        .iter()
        .all(|&e| browsers.has_browser(e) && browsers.host_emit_ready(&e))
    {
        const STALL_WARN_AFTER: std::time::Duration = std::time::Duration::from_millis(1000);
        for &e in &target_list {
            if browsers.has_browser(e) && browsers.host_emit_ready(&e) {
                continue;
            }
            let Ok(t0) = opened.get(e) else {
                continue;
            };
            if t0.0.elapsed() >= STALL_WARN_AFTER && perf.stall_warned.insert(e) {
                bevy::log::warn!(
                    "vmux history: pane {:?} still waiting for CEF host_emit_ready after {:?} (typical cause: ~1MiB WASM compile/init on first navigation)",
                    e,
                    t0.0.elapsed()
                );
            }
        }
        return;
    }

    // Prefer host emit after `vmux_request_history` (listener ready). After a fallback delay with
    // `host_emit_ready`, emit anyway so the UI is not stuck if JS IPC was lost or a pane was recreated.
    if !target_list
        .iter()
        .all(|&e| history_pane_listener_ready(&state, e, &opened, &browsers))
    {
        state.stream = None;
        return;
    }

    // Continue streaming older rows (same revision + panes).
    if let Some(ref mut st) = state.stream {
        if st.revision == hist.revision && st.targets == target_list {
            let n = hist.entries.len();
            if st.next_offset >= n {
                state.stream = None;
                if all_wasm {
                    state.last_revision = Some(hist.revision);
                }
                state.last_target_entities = target_list.clone();
                return;
            }
            let take = HISTORY_STREAM_CHUNK_LEN.min(n - st.next_offset);
            let chunk = history_wire_slice(&hist.entries, st.next_offset, take);
            let done = st.next_offset + take >= n;
            let loaded_after = st.next_offset + take;
            let pct = ((loaded_after as f32 / n as f32) * 100.0).clamp(0.0, 100.0) as u8;
            for wv in target_list.iter().copied() {
                trigger_vmux_history_progress_emit(
                    &mut commands,
                    wv,
                    "stream",
                    format!("Fetching history... ({loaded_after}/{n})"),
                    pct.max(80),
                );
                let payload = HistoryHostPayload {
                    entries: chunk.clone(),
                    sync_nonce: None,
                    history_stream_append: true,
                    history_stream_done: done,
                };
                trigger_vmux_history_emit(&mut commands, &mut *perf, &opened, wv, &payload);
            }
            st.next_offset += take;
            if done {
                state.stream = None;
                if all_wasm {
                    state.last_revision = Some(hist.revision);
                }
            }
            state.last_target_entities = target_list;
            return;
        }
        state.stream = None;
    }

    let pending_sync_for_targets = target_list
        .iter()
        .any(|e| state.pending_history_sync_nonce.contains_key(e));
    if targets_unchanged && revision_unchanged && !pending_sync_for_targets {
        return;
    }

    if !all_wasm {
        let now = Instant::now();
        if let Some(prev) = state.last_fallback_emit_at {
            if now.duration_since(prev) < HISTORY_FALLBACK_EMIT_MIN_INTERVAL {
                return;
            }
        }
        state.last_fallback_emit_at = Some(now);
    }

    let n = hist.entries.len();
    if n == 0 {
        for wv in target_list.iter().copied() {
            trigger_vmux_history_progress_emit(&mut commands, wv, "ready", "History loaded.", 100);
            let sync_nonce = state.pending_history_sync_nonce.remove(&wv);
            let payload = HistoryHostPayload {
                entries: Vec::new(),
                sync_nonce,
                history_stream_append: false,
                history_stream_done: true,
            };
            trigger_vmux_history_emit(&mut commands, &mut *perf, &opened, wv, &payload);
        }
        if all_wasm {
            state.last_revision = Some(hist.revision);
        }
        state.last_target_entities = target_list;
        state.stream = None;
        return;
    }

    let first_len = HISTORY_STREAM_FIRST_LEN.min(n);
    let chunk0 = history_wire_slice(&hist.entries, 0, first_len);
    let done_immediately = first_len >= n;
    let first_pct = ((first_len as f32 / n as f32) * 100.0).clamp(0.0, 100.0) as u8;

    for wv in target_list.iter().copied() {
        trigger_vmux_history_progress_emit(
            &mut commands,
            wv,
            "snapshot",
            format!("Fetching history... ({first_len}/{n})"),
            if done_immediately {
                100
            } else {
                first_pct.max(70)
            },
        );
        let sync_nonce = state.pending_history_sync_nonce.remove(&wv);
        let payload = HistoryHostPayload {
            entries: chunk0.clone(),
            sync_nonce,
            history_stream_append: false,
            history_stream_done: done_immediately,
        };
        trigger_vmux_history_emit(&mut commands, &mut *perf, &opened, wv, &payload);
    }

    if done_immediately {
        if all_wasm {
            state.last_revision = Some(hist.revision);
        }
        state.stream = None;
    } else {
        state.stream = Some(HistoryStreamInFlight {
            revision: hist.revision,
            next_offset: first_len,
            targets: target_list.clone(),
        });
    }
    state.last_target_entities = target_list;
}

/// How [`apply_open_history_pane`] should behave when a history pane may already exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenHistoryMode {
    /// Focus an existing history pane in the layout, or split and open one if none.
    FocusOrOpen,
    /// Always split from the active pane and open a new history pane.
    NewPaneAlways,
}

/// Focus the history pane if it exists (when [`OpenHistoryMode::FocusOrOpen`]), otherwise split in a new history pane.
#[allow(clippy::too_many_arguments)]
pub fn apply_open_history_pane(
    emit_state: &mut HistoryUiEmitState,
    commands: &mut Commands,
    layout_q: &mut Query<&mut Layout, With<vmux_layout::Window>>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    all_panes: &Query<Entity, With<Pane>>,
    active: &Query<Entity, (With<Pane>, With<Active>)>,
    chrome_or_border: &Query<
        (Entity, &PaneChromeOwner),
        Or<(With<PaneChromeStrip>, With<PaneChromeLoadingBar>)>,
    >,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    loading_bar_materials: &mut ResMut<Assets<LoadingBarMaterial>>,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    settings: &VmuxAppSettings,
    history_ui_url: Option<&str>,
    standby_history_pane: Option<Entity>,
    mode: OpenHistoryMode,
) {
    let Ok(active_ent) = active.single() else {
        return;
    };
    let Ok(mut tree) = layout_q.single_mut() else {
        return;
    };

    if matches!(mode, OpenHistoryMode::FocusOrOpen)
        && let Some(hist) = history_panes.iter().find(|e| tree.root.contains_leaf(*e))
    {
        if active_ent == hist {
            // User hit “open history” while already on the history pane; still allow a resync
            // (same race as initial load).
            emit_state.last_revision = None;
            emit_state.stream = None;
            return;
        }
        for p in all_panes.iter() {
            commands.entity(p).remove::<Active>();
        }
        commands.entity(hist).insert(Active);
        return;
    }

    if let Some(standby) = standby_history_pane
        && try_split_active_history_existing_pane(
            commands,
            &mut tree,
            active_ent,
            LayoutAxis::Horizontal,
            standby,
            snapshot,
            pane_last,
            webview_src,
            history_panes,
            path,
            session_queue,
            settings.browser.default_webview_url.as_str(),
        )
    {
        commands.entity(standby).remove::<HistoryPaneStandby>();
        return;
    }
    try_split_active_history_pane(
        commands,
        &mut tree,
        active_ent,
        LayoutAxis::Horizontal,
        chrome_or_border,
        meshes,
        materials,
        loading_bar_materials,
        snapshot,
        pane_last,
        webview_src,
        history_panes,
        path,
        session_queue,
        settings.browser.default_webview_url.as_str(),
        history_ui_url,
    );
}

/// Bundles open-history params so the system stays within Bevy’s system-parameter tuple limit.
#[derive(SystemParam)]
struct OpenHistoryHotkeyAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<WebviewExtendStandardMaterial>>,
    loading_bar_materials: ResMut<'w, Assets<LoadingBarMaterial>>,
    snapshot: ResMut<'w, SessionLayoutSnapshot>,
    session_queue: ResMut<'w, SessionSaveQueue>,
    settings: Res<'w, VmuxAppSettings>,
    hist_ui: Res<'w, HistoryUiBaseUrl>,
    emit_state: ResMut<'w, HistoryUiEmitState>,
}

#[derive(SystemParam)]
struct OpenHistoryHotkeyQueries<'w, 's> {
    state: Query<'w, 's, &'static ActionState<KeyAction>, With<AppInputRoot>>,
    layout_q: Query<'w, 's, &'static mut Layout, With<vmux_layout::Window>>,
    history_panes: Query<'w, 's, Entity, (With<Pane>, With<Webview>, With<History>)>,
    all_panes: Query<'w, 's, Entity, With<Pane>>,
    active: Query<'w, 's, Entity, (With<Pane>, With<Active>)>,
    pane_last: Query<'w, 's, &'static PaneLastUrl>,
    webview_src: Query<'w, 's, &'static WebviewSource>,
    chrome_or_border: Query<
        'w,
        's,
        (Entity, &'static PaneChromeOwner),
        Or<(With<PaneChromeStrip>, With<PaneChromeLoadingBar>)>,
    >,
    standby_history: Query<
        'w,
        's,
        Entity,
        (With<History>, With<Pane>, With<Webview>, With<HistoryPaneStandby>),
    >,
    path: Option<Res<'w, SessionSavePath>>,
}

fn open_history_pane_hotkey(
    mut commands: Commands,
    mut assets: OpenHistoryHotkeyAssets,
    mut queries: OpenHistoryHotkeyQueries,
) {
    let Ok(s) = queries.state.single() else {
        return;
    };
    let mode = if s.just_pressed(&KeyAction::OpenHistoryInNewTab) {
        OpenHistoryMode::NewPaneAlways
    } else if s.just_pressed(&KeyAction::OpenHistory) {
        OpenHistoryMode::FocusOrOpen
    } else {
        return;
    };
    apply_open_history_pane(
        &mut assets.emit_state,
        &mut commands,
        &mut queries.layout_q,
        &queries.history_panes,
        &queries.all_panes,
        &queries.active,
        &queries.chrome_or_border,
        &mut assets.meshes,
        &mut assets.materials,
        &mut assets.loading_bar_materials,
        &mut assets.snapshot,
        &queries.pane_last,
        &queries.webview_src,
        queries.path.as_ref(),
        &mut assets.session_queue,
        &assets.settings,
        assets.hist_ui.0.as_deref(),
        queries.standby_history.iter().next(),
        mode,
    );
}

fn open_history_pane_requested(
    mut requests: ResMut<AppCommandRequestQueue>,
    mut commands: Commands,
    mut assets: OpenHistoryHotkeyAssets,
    mut queries: OpenHistoryHotkeyQueries,
) {
    let mode = if requests.open_history_in_new_tab_requested {
        requests.open_history_in_new_tab_requested = false;
        OpenHistoryMode::NewPaneAlways
    } else if requests.open_history_requested {
        requests.open_history_requested = false;
        OpenHistoryMode::FocusOrOpen
    } else {
        return;
    };
    apply_open_history_pane(
        &mut assets.emit_state,
        &mut commands,
        &mut queries.layout_q,
        &queries.history_panes,
        &queries.all_panes,
        &queries.active,
        &queries.chrome_or_border,
        &mut assets.meshes,
        &mut assets.materials,
        &mut assets.loading_bar_materials,
        &mut assets.snapshot,
        &queries.pane_last,
        &queries.webview_src,
        queries.path.as_ref(),
        &mut assets.session_queue,
        &assets.settings,
        assets.hist_ui.0.as_deref(),
        queries.standby_history.iter().next(),
        mode,
    );
}

fn on_vmux_clear_history(
    trigger: On<Receive<WebviewDocumentUrlEmit>>,
    history: Query<
        (),
        (
            With<Pane>,
            With<Webview>,
            With<History>,
            Without<HistoryPaneStandby>,
        ),
    >,
    mut nav_hist: ResMut<NavigationHistory>,
    nav_path: Res<NavigationHistoryPath>,
    mut nav_queue: ResMut<NavigationHistorySaveQueue>,
) {
    let ev = trigger.event();
    if !ev.vmux_clear_history {
        return;
    }
    if !history.contains(ev.webview) {
        return;
    }
    if nav_hist.clear() {
        nav_queue.0.push(nav_path.0.clone());
    }
}

fn on_vmux_open_in_pane(
    trigger: On<Receive<WebviewDocumentUrlEmit>>,
    history: Query<
        (),
        (
            With<Pane>,
            With<Webview>,
            With<History>,
            Without<HistoryPaneStandby>,
        ),
    >,
    mut commands: Commands,
) {
    let ev = trigger.event();
    let Some(u) = ev.vmux_open_in_pane.as_deref() else {
        return;
    };
    if !history.contains(ev.webview) {
        return;
    }
    let u = u.trim();
    if u.is_empty() || !vmux_layout::allowed_navigation_url(u) {
        return;
    }
    commands.trigger(RequestNavigate {
        webview: ev.webview,
        url: u.to_string(),
    });
}

/// Tiled history pane: navigation payload to the UI, hotkey, and [`VmuxHostedWebPlugin`] surface
/// registration.
#[derive(Default)]
pub struct HistoryUiPlugin;

impl UiPlugin for HistoryUiPlugin {}

impl Plugin for HistoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HistoryUiEmitState>()
            .init_resource::<HistoryPanePerfLog>();
        register_ui_plugin_dioxus_warmup::<Self>(app);
        app.add_observer(on_vmux_open_in_pane)
            .add_observer(on_vmux_clear_history)
            .add_observer(on_vmux_request_history)
            .add_systems(
                Update,
                (
                    poll_history_url.in_set(DioxusUiWarmupSet::PollUrls),
                    spawn_history_pane_standby
                        .after(dioxus_embedded_warmup_system)
                        .in_set(DioxusUiWarmupSet::Warmup),
                    timeout_history_embedded.after(poll_history_url),
                    open_history_pane_hotkey,
                    open_history_pane_requested.after(open_history_pane_hotkey),
                    apply_history_url_to_panes
                        .after(poll_history_url)
                        .after(timeout_history_embedded),
                    invalidate_history_emit_when_focusing_history_pane
                        .after(apply_history_url_to_panes),
                ),
            )
            // After `JsEmitEventPlugin::receive_events` (Update) so `vmux_request_history` clears
            // dedupe state before we push the list — avoids waiting an extra frame for history rows.
            .add_systems(
                PostUpdate,
                (
                    history_emit_reset_on_navigation_or_targets_changed,
                    nudge_history_emit_for_osr_wasm_timers
                        .after(history_emit_reset_on_navigation_or_targets_changed)
                        .after(apply_history_url_to_panes)
                        .after(invalidate_history_emit_when_focusing_history_pane)
                        .after(sync_cef_osr_focus_with_active_pane),
                    nudge_history_emit_when_history_host_emit_ready_rises
                        .after(sync_cef_osr_focus_with_active_pane)
                        .after(nudge_history_emit_for_osr_wasm_timers),
                    emit_history_to_panes
                        .after(history_emit_reset_on_navigation_or_targets_changed)
                        .after(apply_history_url_to_panes)
                        .after(invalidate_history_emit_when_focusing_history_pane)
                        .after(sync_cef_osr_focus_with_active_pane)
                        .after(nudge_history_emit_for_osr_wasm_timers)
                        .after(nudge_history_emit_when_history_host_emit_ready_rises),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_server::ServerPlugin;

    #[test]
    fn history_ui_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins((ServerPlugin, HistoryUiPlugin));
    }
}
