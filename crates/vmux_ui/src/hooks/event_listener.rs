//! [`window.cef.listen`](https://not-elm.github.io/bevy_cef/communication/) + RON host payloads, and [`use_event_listener`] for Dioxus.
//! `HostEmitEvent::new` JSON-stringifies the RON body for the CEF bridge; JS → Bevy uses [`try_cef_emit`].

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use dioxus::prelude::*;
use js_sys::Function;
use serde::de::DeserializeOwned;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::window;

/// Why `cef.listen` / `cef.emit` could not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventListenerError {
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

impl fmt::Display for EventListenerError {
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

fn window_cef() -> Result<JsValue, EventListenerError> {
    let Some(win) = window() else {
        return Err(EventListenerError::NoWindow);
    };
    let Ok(cef) = js_sys::Reflect::get(&win, &JsValue::from_str("cef")) else {
        return Err(EventListenerError::NoCefGlobal);
    };
    if cef.is_null() || cef.is_undefined() {
        return Err(EventListenerError::CefNotInjected);
    }
    Ok(cef)
}

/// Host emit delivers a JSON-encoded string body; inner text is RON for `T`.
fn host_emit_js_to_ron_str(e: JsValue) -> Option<String> {
    let s = e.as_string().filter(|s| !s.is_empty()).or_else(|| {
        js_sys::JSON::stringify(&e)
            .ok()
            .and_then(|j| j.as_string())
            .filter(|s| !s.is_empty())
    })?;
    Some(s)
}

fn cef_emit_fn(cef: &JsValue) -> Result<Function, EventListenerError> {
    let Ok(emit) = js_sys::Reflect::get(cef, &JsValue::from_str("emit")) else {
        return Err(EventListenerError::NoEmitMethod);
    };
    emit.dyn_into::<Function>()
        .map_err(|_| EventListenerError::EmitNotCallable)
}

/// Call `cef.emit(payload)`. Bevy’s JS Emit path forwards this value as one JSON blob to the host.
pub fn try_cef_emit(payload: &JsValue) -> Result<(), EventListenerError> {
    let cef = window_cef()?;
    let emit_fn = cef_emit_fn(&cef)?;
    let _ = emit_fn.call1(&cef, payload);
    Ok(())
}

/// Serialize with `serde_json`, parse in JS, then [`try_cef_emit`].
#[allow(dead_code)]
pub fn try_cef_emit_serde<T: serde::Serialize>(payload: &T) -> Result<(), EventListenerError> {
    let json = serde_json::to_string(payload).map_err(|_| EventListenerError::SerializePayload)?;
    let value = js_sys::JSON::parse(&json).map_err(|_| EventListenerError::InvalidJson)?;
    try_cef_emit(&value)
}

/// Register `cef.listen` when `window.cef` exists. Each host payload is RON text (after JSON string unwrap), then deserialized as `T` before `on_event`.
pub fn try_cef_listen<T, F>(name: &str, on_event: F) -> Result<(), EventListenerError>
where
    T: DeserializeOwned + 'static,
    F: FnMut(T) + 'static,
{
    let cef = window_cef()?;
    let Ok(listen) = js_sys::Reflect::get(&cef, &JsValue::from_str("listen")) else {
        return Err(EventListenerError::NoListenMethod);
    };
    let Ok(listen_fn) = listen.dyn_into::<Function>() else {
        return Err(EventListenerError::ListenNotCallable);
    };

    let mut on_event = on_event;
    let closure = Closure::wrap(Box::new(move |e: JsValue| {
        if let Some(ron_str) = host_emit_js_to_ron_str(e)
            && let Ok(msg) = ron::de::from_str::<T>(&ron_str)
        {
            on_event(msg);
        }
    }) as Box<dyn FnMut(JsValue)>);

    let cb = closure.as_ref().unchecked_ref();
    let _ = listen_fn.call2(&cef, &JsValue::from_str(name), cb);
    closure.forget();
    Ok(())
}

/// Emit `{}` for JS Emit after the host listener is registered (handshake with Bevy `JsEmitEventPlugin`).
pub fn try_emit_ui_ready() -> Result<(), EventListenerError> {
    try_cef_emit(&JsValue::from(js_sys::Object::new()))
}

/// Subscribed host stream: listener registration flag, optional setup error, and last payload.
pub struct BevyState<T> {
    /// `true` until `cef.listen` succeeds (listener not registered yet). `false` after that or if listen fails.
    pub is_loading: Signal<bool>,
    /// Listen or emit-ready failure (`None` if no error yet).
    pub error: Signal<Option<String>>,
    /// Latest deserialized value from the host for this channel (`None` until the first message).
    #[allow(dead_code)]
    pub state: Signal<Option<T>>,
}

/// Register `cef.listen` on `name`, deserialize each payload as `T` from RON, update [`BevyState::state`],
/// then call `on_event` with the same value.
/// After a successful listen, emits UI ready via [`try_emit_ui_ready`].
pub fn use_event_listener<T, F>(name: &'static str, on_event: F) -> BevyState<T>
where
    T: DeserializeOwned + Clone + 'static,
    F: FnMut(T) + 'static,
{
    let on_event = Rc::new(RefCell::new(on_event));
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut state = use_signal(|| None::<T>);

    use_hook(move || {
        let on_event = Rc::clone(&on_event);
        match try_cef_listen::<T, _>(name, move |msg| {
            state.set(Some(msg.clone()));
            on_event.borrow_mut()(msg);
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
