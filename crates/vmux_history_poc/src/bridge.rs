//! CEF IPC channel id for [Host Emit](https://not-elm.github.io/bevy_cef/communication/) (Bevy → JS).
//! JS → Bevy uses `cef.emit` with a single JSON value ([`try_cef_emit`]).

use std::fmt;

/// `window.cef.listen` / host `HostEmitEvent` channel for the history JSON payload.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub const HOST_HISTORY_CHANNEL: &str = "vmux_history_poc_history";

/// Why `cef.listen` / `cef.emit` could not run (WASM UI only; unused on native host builds).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CefBridgeError {
    NoWindow,
    NoCefGlobal,
    CefNotInjected,
    NoListenMethod,
    ListenNotCallable,
    NoEmitMethod,
    EmitNotCallable,
    /// Returned only from [`try_cef_emit_serde`].
    #[allow(dead_code)]
    SerializePayload,
    /// Returned only from [`try_cef_emit_serde`].
    #[allow(dead_code)]
    InvalidJson,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl fmt::Display for CefBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoWindow => "no `window`",
            Self::NoCefGlobal => "no `window.cef` property",
            Self::CefNotInjected => "`window.cef` not ready",
            Self::NoListenMethod => "no `cef.listen`",
            Self::ListenNotCallable => "`cef.listen` is not a function",
            Self::NoEmitMethod => "no `cef.emit`",
            Self::EmitNotCallable => "`cef.emit` is not a function",
            Self::SerializePayload => "failed to serialize emit payload",
            Self::InvalidJson => "`JSON.parse` failed for emit payload",
        })
    }
}

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use js_sys::Function;
#[cfg(target_arch = "wasm32")]
use serde::de::DeserializeOwned;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;
#[cfg(target_arch = "wasm32")]
use web_sys::window;

#[cfg(target_arch = "wasm32")]
fn window_cef() -> Result<JsValue, CefBridgeError> {
    let Some(win) = window() else {
        return Err(CefBridgeError::NoWindow);
    };
    let Ok(cef) = js_sys::Reflect::get(&win, &JsValue::from_str("cef")) else {
        return Err(CefBridgeError::NoCefGlobal);
    };
    if cef.is_null() || cef.is_undefined() {
        return Err(CefBridgeError::CefNotInjected);
    }
    Ok(cef)
}

#[cfg(target_arch = "wasm32")]
fn host_event_js_to_value(e: JsValue) -> serde_json::Value {
    let json_str = js_sys::JSON::stringify(&e)
        .ok()
        .and_then(|s| s.as_string())
        .filter(|s| !s.is_empty())
        .or_else(|| e.as_string())
        .unwrap_or_default();
    if json_str.is_empty() {
        return serde_json::Value::Null;
    }
    serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
}

#[cfg(target_arch = "wasm32")]
fn cef_emit_fn(cef: &JsValue) -> Result<Function, CefBridgeError> {
    let Ok(emit) = js_sys::Reflect::get(cef, &JsValue::from_str("emit")) else {
        return Err(CefBridgeError::NoEmitMethod);
    };
    emit.dyn_into::<Function>()
        .map_err(|_| CefBridgeError::EmitNotCallable)
}

/// Call `cef.emit(payload)`. Bevy’s JS Emit path forwards this value as one JSON blob to the host.
#[cfg(target_arch = "wasm32")]
pub fn try_cef_emit(payload: &JsValue) -> Result<(), CefBridgeError> {
    let cef = window_cef()?;
    let emit_fn = cef_emit_fn(&cef)?;
    let _ = emit_fn.call1(&cef, payload);
    Ok(())
}

/// Serialize with `serde_json`, parse in JS, then [`try_cef_emit`].
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)] // Typed emits from the UI; [`try_cef_emit`] is enough when you already have a [`JsValue`].
pub fn try_cef_emit_serde<T: serde::Serialize>(payload: &T) -> Result<(), CefBridgeError> {
    let json = serde_json::to_string(payload).map_err(|_| CefBridgeError::SerializePayload)?;
    let value = js_sys::JSON::parse(&json).map_err(|_| CefBridgeError::InvalidJson)?;
    try_cef_emit(&value)
}

/// Register `cef.listen` when `window.cef` exists. Each host payload is parsed from JS into JSON, then deserialized as `T` before `on_event`.
#[cfg(target_arch = "wasm32")]
pub fn try_cef_listen<T, F>(channel: &str, on_event: F) -> Result<(), CefBridgeError>
where
    T: DeserializeOwned + 'static,
    F: FnMut(T) + 'static,
{
    let cef = window_cef()?;
    let Ok(listen) = js_sys::Reflect::get(&cef, &JsValue::from_str("listen")) else {
        return Err(CefBridgeError::NoListenMethod);
    };
    let Ok(listen_fn) = listen.dyn_into::<Function>() else {
        return Err(CefBridgeError::ListenNotCallable);
    };

    let mut on_event = on_event;
    let closure = Closure::wrap(Box::new(move |e: JsValue| {
        let v = host_event_js_to_value(e);
        if let Ok(msg) = serde_json::from_value::<T>(v) {
            on_event(msg);
        }
    }) as Box<dyn FnMut(JsValue)>);

    let cb = closure.as_ref().unchecked_ref();
    let _ = listen_fn.call2(&cef, &JsValue::from_str(channel), cb);
    closure.forget();
    Ok(())
}

/// Emit `{}` for JS Emit after the host-history listener is registered (matches `HistoryUiReady` on the Bevy side).
#[cfg(target_arch = "wasm32")]
pub fn try_emit_ui_ready() -> Result<(), CefBridgeError> {
    try_cef_emit(&JsValue::from(js_sys::Object::new()))
}

/// Subscribed host stream: listener registration flag, optional setup error, and last payload from Bevy.
#[cfg(target_arch = "wasm32")]
pub struct BevyState<T> {
    /// `true` until `cef.listen` succeeds (listener not registered yet). `false` after that or if listen fails.
    pub is_loading: Signal<bool>,
    /// Listen or emit-ready failure (`None` if no error yet).
    pub error: Signal<Option<String>>,
    /// Latest deserialized value from the host for this channel (`None` until the first message).
    pub state: Signal<Option<T>>,
}

/// Register `cef.listen` on `channel`, deserialize each payload as `T`, update [`BevyState::state`],
/// then call `on_message` with the same value.
/// After a successful listen, emits UI ready via [`try_emit_ui_ready`] (POC handshake with Bevy).
#[cfg(target_arch = "wasm32")]
pub fn use_core_bridge<T, F>(channel: &'static str, on_message: F) -> BevyState<T>
where
    T: DeserializeOwned + Clone + 'static,
    F: FnMut(T) + 'static,
{
    let on_message = Rc::new(RefCell::new(on_message));
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut state = use_signal(|| None::<T>);

    use_hook(move || {
        let on_message = Rc::clone(&on_message);
        match try_cef_listen::<T, _>(channel, move |msg| {
            state.set(Some(msg.clone()));
            on_message.borrow_mut()(msg);
        }) {
            Ok(()) => {
                is_loading.set(false);
                match try_emit_ui_ready() {
                    Ok(()) => {}
                    Err(e) => error.set(Some(format!("cef.emit failed: {e}"))),
                }
            }
            Err(e) => {
                is_loading.set(false);
                error.set(Some(format!("cef.listen failed: {e}")));
            }
        }
    });

    BevyState {
        is_loading,
        error,
        state,
    }
}
