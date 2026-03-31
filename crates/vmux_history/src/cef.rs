//! CEF host IPC via `web_sys` / `wasm_bindgen::Closure` (no hand-written JavaScript).

use dioxus::prelude::Signal;
use futures_channel::mpsc::UnboundedSender;
use js_sys::Function;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::window;

use crate::payload::{BridgeMsg, HistoryEntryWire, apply_history_payload};

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
    let Some(win) = window() else {
        return false;
    };
    let Ok(cef) = js_sys::Reflect::get(&win, &JsValue::from_str("cef")) else {
        return false;
    };
    if cef.is_null() || cef.is_undefined() {
        return false;
    }
    let Ok(listen) = js_sys::Reflect::get(&cef, &JsValue::from_str("listen")) else {
        return false;
    };
    let Ok(listen_fn) = listen.dyn_into::<Function>() else {
        return false;
    };

    let closure = Closure::wrap(Box::new(move |e: JsValue| {
        let payload = serde_wasm_bindgen::from_value(e).unwrap_or(serde_json::Value::Null);
        let msg = serde_json::json!({ "type": "history", "payload": payload });
        let _ = tx.unbounded_send(msg);
    }) as Box<dyn FnMut(JsValue)>);

    let cb = closure.as_ref().unchecked_ref();
    let _ = listen_fn.call2(&cef, &JsValue::from_str("vmux_history"), cb);
    closure.forget();
    true
}

/// Ask Bevy to send the current history list again (covers host emit before `listen` was registered).
///
/// `sync_nonce` is echoed on the next `vmux_history` payload so the UI can confirm delivery.
pub fn request_history_sync_from_host(sync_nonce: Option<u32>) {
    let Some(win) = window() else {
        return;
    };
    let Ok(cef) = js_sys::Reflect::get(&win, &JsValue::from_str("cef")) else {
        return;
    };
    if cef.is_null() || cef.is_undefined() {
        return;
    }
    let Ok(emit) = js_sys::Reflect::get(&cef, &JsValue::from_str("emit")) else {
        return;
    };
    let Ok(emit_fn) = emit.dyn_into::<Function>() else {
        return;
    };
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("vmux_request_history"),
        &JsValue::TRUE,
    );
    if let Some(n) = sync_nonce {
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("vmux_history_sync_nonce"),
            &JsValue::from_f64(f64::from(n)),
        );
    }
    let _ = emit_fn.call1(&cef, &obj);
}

pub fn emit_open_in_pane(url: &str) {
    let Some(win) = window() else {
        return;
    };
    let Ok(cef) = js_sys::Reflect::get(&win, &JsValue::from_str("cef")) else {
        return;
    };
    if cef.is_null() || cef.is_undefined() {
        return;
    }
    let Ok(emit) = js_sys::Reflect::get(&cef, &JsValue::from_str("emit")) else {
        return;
    };
    let Ok(emit_fn) = emit.dyn_into::<Function>() else {
        return;
    };
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("vmux_open_in_pane"),
        &JsValue::from_str(url),
    );
    let _ = emit_fn.call1(&cef, &obj);
}

pub fn emit_clear_history() {
    let Some(win) = window() else {
        return;
    };
    let Ok(cef) = js_sys::Reflect::get(&win, &JsValue::from_str("cef")) else {
        return;
    };
    if cef.is_null() || cef.is_undefined() {
        return;
    }
    let Ok(emit) = js_sys::Reflect::get(&cef, &JsValue::from_str("emit")) else {
        return;
    };
    let Ok(emit_fn) = emit.dyn_into::<Function>() else {
        return;
    };
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("vmux_clear_history"),
        &JsValue::TRUE,
    );
    let _ = emit_fn.call1(&cef, &obj);
}

pub async fn run_history_bridge_loop(
    mut rx: futures_channel::mpsc::UnboundedReceiver<serde_json::Value>,
    entries: Signal<Vec<HistoryEntryWire>>,
    bridge_sync_pending: Signal<Option<u32>>,
    host_snapshot_received: Signal<bool>,
    history_stream_complete: Signal<bool>,
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
        }
    }
}
