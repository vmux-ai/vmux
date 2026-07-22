use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, SnapshotResult};
use vmux_agent::{BrowserSnapshotRequest, BrowserSnapshotResponse, NavAwaitingSnapshot};
use vmux_browser::PendingNavSnapshots;
use vmux_core::LastActivatedAt;
use vmux_core::dom_snapshot::{RawSnapshot, shape_snapshot};
use vmux_core::terminal::{ProcessExited, Terminal};
use vmux_layout::active_panes::ActivePanes;
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_layout::stack::{Stack, active_stack_in_pane};
use vmux_layout::target::active_webview_for_tab;
use vmux_layout::{Browser, Loading};

fn hex(id: &[u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    for b in id {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn parse_hex(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_snapshots(
    mut reader: MessageReader<BrowserSnapshotRequest>,
    cef_browsers: NonSend<Browsers>,
    active: Res<ActivePanes>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<Entity, With<Stack>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut writer: MessageWriter<BrowserSnapshotResponse>,
) {
    for request in reader.read() {
        let webview = request
            .webview
            .filter(|webview| browsers.contains(*webview))
            .or_else(|| {
                let target = request.pane.as_deref().and_then(|target| {
                    vmux_layout::target::parse_browser_target(target, &panes, &stacks)
                })?;
                vmux_layout::target::webview_for_target(
                    target,
                    &pane_children,
                    &stack_ts,
                    &browsers,
                    &terminals,
                )
            })
            .or_else(|| {
                active
                    .local()
                    .pane
                    .filter(|p| panes.contains(*p))
                    .and_then(|pane| {
                        active_webview_for_tab(
                            active_stack_in_pane(pane, &pane_children, &stack_ts),
                            &browsers,
                            &terminals,
                        )
                    })
            })
            .or_else(|| most_recent_browser(&browsers, &terminals, &stack_ts));
        let sent = webview
            .map(|webview| cef_browsers.request_snapshot(&webview, &hex(&request.request_id)))
            .unwrap_or(false);
        if !sent {
            writer.write(BrowserSnapshotResponse {
                request_id: request.request_id,
                result: Err("no browser page to snapshot".to_string()),
            });
        }
    }
}

/// The browser webview whose stack was activated most recently — a fallback when the requested
/// pane is missing/stale or holds no browser (e.g. an ACP agent passed a bogus pane id, or its own
/// anchor pane is a chat/terminal). Keeps `browser_snapshot` from hard-failing while a browser the
/// user can see is open somewhere.
pub(crate) fn most_recent_browser(
    browsers: &Query<(Entity, &ChildOf), With<Browser>>,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
) -> Option<Entity> {
    browsers
        .iter()
        .filter_map(|(entity, child_of)| {
            if terminals.iter().any(|(t, _)| t == entity) {
                return None;
            }
            let (_, ts) = stack_ts.get(child_of.get()).ok()?;
            Some((entity, ts.0))
        })
        .max_by_key(|&(_, ts)| ts)
        .map(|(entity, _)| entity)
}

pub(crate) fn drive_pending_nav_snapshots(
    time: Res<Time>,
    mut pending: ResMut<PendingNavSnapshots>,
    loading_q: Query<(), With<Loading>>,
    alive_q: Query<(), With<Browser>>,
    ready_q: Query<(), With<vmux_core::page::PageReady>>,
    mut nav_awaiting: ResMut<NavAwaitingSnapshot>,
    mut snapshot_writer: MessageWriter<BrowserSnapshotRequest>,
) {
    if pending.0.is_empty() {
        return;
    }
    let now = time.elapsed();
    let mut done: Vec<Entity> = Vec::new();
    for (webview, nav) in pending.0.iter_mut() {
        let alive = alive_q.contains(*webview);
        let ready = ready_q.contains(*webview);
        let loading = loading_q.contains(*webview);
        if loading {
            nav.saw_loading = true;
        }
        let elapsed = now.saturating_sub(nav.started).as_secs_f32();
        let settled = nav.saw_loading && !loading;
        let assume_instant = !nav.saw_loading && elapsed > 2.0;
        let timed_out = elapsed > 10.0;
        if !alive || ready && (settled || assume_instant) || timed_out {
            nav_awaiting.0.insert(nav.request_id);
            snapshot_writer.write(BrowserSnapshotRequest {
                request_id: nav.request_id,
                pane: nav.pane.clone(),
                webview: Some(*webview),
            });
            done.push(*webview);
        }
    }
    for webview in done {
        pending.0.remove(&webview);
    }
}

pub(crate) fn shape_snapshot_results(
    mut reader: MessageReader<SnapshotResult>,
    mut writer: MessageWriter<BrowserSnapshotResponse>,
) {
    for result in reader.read() {
        let Some(request_id) = parse_hex(&result.request_id) else {
            continue;
        };
        let mapped = serde_json::from_str::<RawSnapshot>(&result.json)
            .map(|raw| serde_json::to_string(&shape_snapshot(raw)).unwrap_or_default())
            .map_err(|e| format!("snapshot parse error: {e}"));
        writer.write(BrowserSnapshotResponse {
            request_id,
            result: mapped,
        });
    }
}
