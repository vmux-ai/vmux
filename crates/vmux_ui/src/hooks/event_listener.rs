use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;
use js_sys::Function;
use serde::de::DeserializeOwned;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::window;

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

fn decode_host_emit_js<T: DeserializeOwned>(e: &JsValue) -> Option<T> {
    if let Some(s) = e.as_string().filter(|t| !t.is_empty()) {
        if let Ok(v) = ron::de::from_str::<T>(&s) {
            return Some(v);
        }
        if let Ok(v) = serde_json::from_str::<T>(&s) {
            return Some(v);
        }
        return None;
    }
    let json = js_sys::JSON::stringify(e).ok()?;
    let s = json.as_string()?;
    serde_json::from_str(&s).ok()
}

fn cef_emit_fn(cef: &JsValue) -> Result<Function, EventListenerError> {
    let Ok(emit) = js_sys::Reflect::get(cef, &JsValue::from_str("emit")) else {
        return Err(EventListenerError::NoEmitMethod);
    };
    emit.dyn_into::<Function>()
        .map_err(|_| EventListenerError::EmitNotCallable)
}

pub fn try_cef_emit(payload: &JsValue) -> Result<(), EventListenerError> {
    let cef = window_cef()?;
    let emit_fn = cef_emit_fn(&cef)?;
    let _ = emit_fn.call1(&cef, payload);
    Ok(())
}

#[allow(dead_code)]
pub fn try_cef_emit_serde<T: serde::Serialize>(payload: &T) -> Result<(), EventListenerError> {
    let json = serde_json::to_string(payload).map_err(|_| EventListenerError::SerializePayload)?;
    let value = js_sys::JSON::parse(&json).map_err(|_| EventListenerError::InvalidJson)?;
    try_cef_emit(&value)
}

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
        if let Some(msg) = decode_host_emit_js::<T>(&e) {
            on_event(msg);
        }
    }) as Box<dyn FnMut(JsValue)>);

    let cb = closure.as_ref().unchecked_ref();
    let _ = listen_fn.call2(&cef, &JsValue::from_str(name), cb);
    closure.forget();
    Ok(())
}

pub fn try_emit_ui_ready() -> Result<(), EventListenerError> {
    try_cef_emit(&JsValue::from(js_sys::Object::new()))
}

pub struct BevyState {
    pub is_loading: Signal<bool>,
    pub error: Signal<Option<String>>,
}

pub fn use_event_listener<T, F>(name: &'static str, on_event: F) -> BevyState
where
    T: DeserializeOwned + 'static,
    F: FnMut(T) + 'static,
{
    let on_event = Rc::new(RefCell::new(on_event));
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    use_hook(move || {
        let on_event = Rc::clone(&on_event);
        let Some(rt) = Runtime::try_current() else {
            is_loading.set(false);
            error.set(Some(
                "use_event_listener: no Dioxus runtime (internal error)".into(),
            ));
            return;
        };
        let scope = current_scope_id();
        match try_cef_listen::<T, _>(name, move |msg| {
            let on_event = Rc::clone(&on_event);
            rt.in_scope(scope, || {
                on_event.borrow_mut()(msg);
            });
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

    BevyState { is_loading, error }
}

/// Decode a base64-encoded rkyv payload from a CEF host-emit event.
fn decode_rkyv_host_emit<T>(e: &JsValue) -> Option<T>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
{
    let s = e.as_string()?;
    let bytes = BASE64.decode(s.as_bytes()).ok()?;
    // SAFETY: bytes are produced by our own rkyv::to_bytes on the native side.
    unsafe { rkyv::from_bytes_unchecked::<T, rkyv::rancor::Error>(&bytes).ok() }
}

/// Like [`use_event_listener`] but decodes the payload using rkyv + base64
/// instead of RON/JSON serde.
pub fn use_rkyv_event_listener<T, F>(name: &'static str, on_event: F) -> BevyState
where
    T: rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
    F: FnMut(T) + 'static,
{
    let on_event = Rc::new(RefCell::new(on_event));
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    use_hook(move || {
        let on_event = Rc::clone(&on_event);
        let Some(rt) = Runtime::try_current() else {
            is_loading.set(false);
            error.set(Some(
                "use_rkyv_event_listener: no Dioxus runtime (internal error)".into(),
            ));
            return;
        };
        let scope = current_scope_id();

        let result = (|| -> Result<(), EventListenerError> {
            let cef = window_cef()?;
            let Ok(listen) = js_sys::Reflect::get(&cef, &JsValue::from_str("listen")) else {
                return Err(EventListenerError::NoListenMethod);
            };
            let Ok(listen_fn) = listen.dyn_into::<Function>() else {
                return Err(EventListenerError::ListenNotCallable);
            };

            let on_event_inner = Rc::clone(&on_event);
            let closure = Closure::wrap(Box::new(move |e: JsValue| {
                if let Some(msg) = decode_rkyv_host_emit::<T>(&e) {
                    let on_event = Rc::clone(&on_event_inner);
                    rt.in_scope(scope, || {
                        on_event.borrow_mut()(msg);
                    });
                }
            }) as Box<dyn FnMut(JsValue)>);

            let cb = closure.as_ref().unchecked_ref();
            let _ = listen_fn.call2(&cef, &JsValue::from_str(name), cb);
            closure.forget();
            Ok(())
        })();

        match result {
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

    BevyState { is_loading, error }
}
