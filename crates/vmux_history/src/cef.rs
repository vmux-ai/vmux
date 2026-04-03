//! CEF host IPC via `web_sys` / `wasm_bindgen::Closure` (no hand-written JavaScript).

use dioxus::prelude::Signal;
use futures_channel::mpsc::UnboundedSender;
use wasm_bindgen::JsValue;

use vmux_ui::cef_bridge::{try_cef_emit_keyed, try_cef_listen};

use crate::payload::{
    BridgeMsg, HistoryEntryWire, apply_history_payload, apply_history_progress_payload,
};

/// Random `u32` for [`request_history_sync_from_host`]; stays exact through JS `Number`.
pub fn random_history_sync_nonce() -> u32 {
    let a = (js_sys::Math::random() * f64::from(u32::MAX)) as u32;
    let b = (js_sys::Math::random() * f64::from(u32::MAX)) as u32;
    a ^ b
}

/// Registers `cef.listen("vmux_history", …)` when `window.cef` is ready.
///
/// Call again after creating a new channel so the callback always targets the active `tx` (Dioxus
/// effects can rerun; `cef.listen` overwrites the previous handler for `vmux_history`).
///
/// Returns `false` while `cef` is still booting so callers can retry (host emit can arrive before
/// the listener exists).
pub fn try_install_cef_history_listener(tx: UnboundedSender<serde_json::Value>) -> bool {
    let ok1 = try_cef_listen("vmux_history", {
        let tx = tx.clone();
        move |e: JsValue| {
            let payload = serde_wasm_bindgen::from_value(e).unwrap_or(serde_json::Value::Null);
            let msg = serde_json::json!({ "type": "history", "payload": payload });
            let _ = tx.unbounded_send(msg);
        }
    });
    let ok2 = try_cef_listen("vmux_history_progress", {
        let tx = tx.clone();
        move |e: JsValue| {
            let payload = serde_wasm_bindgen::from_value(e).unwrap_or(serde_json::Value::Null);
            let msg = serde_json::json!({ "type": "progress", "payload": payload });
            let _ = tx.unbounded_send(msg);
        }
    });
    ok1 && ok2
}

/// Ask Bevy to send the current history list again (covers host emit before `listen` was registered).
///
/// `sync_nonce` is echoed on the next `vmux_history` payload so the UI can confirm delivery.
pub fn request_history_sync_from_host(sync_nonce: Option<u32>) {
    let mut pairs = vec![
        ("vmux_request_history", JsValue::TRUE),
    ];
    if let Some(n) = sync_nonce {
        pairs.push((
            "vmux_history_sync_nonce",
            JsValue::from_f64(f64::from(n)),
        ));
    }
    let _ = try_cef_emit_keyed(&pairs);
}

pub fn emit_open_in_pane(url: &str) {
    let _ = try_cef_emit_keyed(&[("vmux_open_in_pane", JsValue::from_str(url))]);
}

pub fn emit_clear_history() {
    let _ = try_cef_emit_keyed(&[("vmux_clear_history", JsValue::TRUE)]);
}

pub async fn run_history_bridge_loop(
    mut rx: futures_channel::mpsc::UnboundedReceiver<serde_json::Value>,
    entries: Signal<Vec<HistoryEntryWire>>,
    bridge_sync_pending: Signal<Option<u32>>,
    host_snapshot_received: Signal<bool>,
    history_stream_complete: Signal<bool>,
    progress_stage: Signal<String>,
    progress_message: Signal<String>,
    progress_percent: Signal<u8>,
) {
    use futures_util::StreamExt;

    while let Some(raw) = rx.next().await {
        let msg: BridgeMsg = match serde_json::from_value(raw) {
            Ok(m) => m,
            Err(_) => continue,
        };
        match msg {
            BridgeMsg::History { payload } => {
                apply_history_payload(
                    payload,
                    entries,
                    bridge_sync_pending,
                    host_snapshot_received,
                    history_stream_complete,
                );
            }
            BridgeMsg::Progress { payload } => {
                apply_history_progress_payload(
                    payload,
                    progress_stage,
                    progress_message,
                    progress_percent,
                );
            }
        }
    }
}
