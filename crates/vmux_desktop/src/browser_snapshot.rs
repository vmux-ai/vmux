use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, SnapshotResult};
use vmux_agent::{BrowserSnapshotRequest, BrowserSnapshotResponse};
use vmux_core::LastActivatedAt;
use vmux_core::dom_snapshot::{RawSnapshot, shape_snapshot};
use vmux_core::terminal::{ProcessExited, Terminal};
use vmux_layout::Browser;
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_layout::stack::{FocusedStack, Stack, active_stack_in_pane};
use vmux_layout::target::{active_webview_for_tab, parse_pane_target};

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
    focus: Res<FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut writer: MessageWriter<BrowserSnapshotResponse>,
) {
    for request in reader.read() {
        let target = match request.pane.as_deref() {
            Some(s) => parse_pane_target(s, &panes),
            None => focus.pane.filter(|p| panes.contains(*p)),
        };
        let webview = target.and_then(|pane| {
            active_webview_for_tab(
                active_stack_in_pane(pane, &pane_children, &stack_ts),
                &browsers,
                &terminals,
            )
        });
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
