use std::fmt;

use crate::bin_ipc_envelope::encode_bin_ipc_envelope;
use crate::listener_guard::GuardedListener;
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;
use js_sys::Function;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::window;

const PAGE_READY_BIN_EVENT_ID: &str = "vmux-page-ready";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventListenerError {
    NoWindow,
    NoCefGlobal,
    CefNotInjected,
    NoListenMethod,
    ListenNotCallable,
    NoEmitMethod,
    EmitNotCallable,
    SerializePayload,
}

impl fmt::Display for EventListenerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoWindow => "no `window`",
            Self::NoCefGlobal => "no `window.cef` property",
            Self::CefNotInjected => "`window.cef` not ready",
            Self::NoListenMethod => "no `cef.binListen`",
            Self::ListenNotCallable => "`cef.binListen` is not a function",
            Self::NoEmitMethod => "no `cef.binEmit`",
            Self::EmitNotCallable => "`cef.binEmit` is not a function",
            Self::SerializePayload => "failed to serialize emit payload",
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

pub fn decode_bin_host_emit_js<T>(e: &JsValue) -> Option<T>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    use js_sys::{ArrayBuffer, Uint8Array};

    let buffer: ArrayBuffer = if let Some(buf) = e.dyn_ref::<ArrayBuffer>() {
        buf.clone()
    } else if let Some(arr) = e.dyn_ref::<Uint8Array>() {
        arr.buffer()
    } else {
        return None;
    };

    let view = Uint8Array::new(&buffer);
    let mut bytes = vec![0u8; view.length() as usize];
    view.copy_to(&mut bytes);

    rkyv::from_bytes::<T, rkyv::rancor::Error>(&bytes).ok()
}

fn cef_bin_emit_fn(cef: &JsValue) -> Result<Function, EventListenerError> {
    let Ok(emit) = js_sys::Reflect::get(cef, &JsValue::from_str("binEmit")) else {
        return Err(EventListenerError::NoEmitMethod);
    };
    emit.dyn_into::<Function>()
        .map_err(|_| EventListenerError::EmitNotCallable)
}

pub fn try_cef_bin_emit_rkyv<T>(payload: &T) -> Result<(), EventListenerError>
where
    T: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
{
    use js_sys::{ArrayBuffer, Uint8Array};

    let cef = window_cef()?;
    let emit_fn = cef_bin_emit_fn(&cef)?;

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(payload)
        .map_err(|_| EventListenerError::SerializePayload)?;

    let envelope = encode_bin_ipc_envelope(std::any::type_name::<T>(), &bytes);
    let buffer = ArrayBuffer::new(envelope.len() as u32);
    let view = Uint8Array::new(&buffer);
    view.copy_from(&envelope);

    let _ = emit_fn.call1(&cef, &buffer.into());
    Ok(())
}

fn cef_bin_listen_fn(cef: &JsValue) -> Result<Function, EventListenerError> {
    let Ok(listen) = js_sys::Reflect::get(cef, &JsValue::from_str("binListen")) else {
        return Err(EventListenerError::NoListenMethod);
    };
    listen
        .dyn_into::<Function>()
        .map_err(|_| EventListenerError::ListenNotCallable)
}

pub fn try_cef_bin_listen<T, F>(name: &str, on_event: F) -> Result<(), EventListenerError>
where
    T: rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    F: FnMut(T) + 'static,
{
    let cef = window_cef()?;
    let listen_fn = cef_bin_listen_fn(&cef)?;

    let mut on_event = on_event;
    let closure = Closure::wrap(Box::new(move |e: JsValue| {
        if let Some(msg) = decode_bin_host_emit_js::<T>(&e) {
            on_event(msg);
        }
    }) as Box<dyn FnMut(JsValue)>);

    let cb = closure.as_ref().unchecked_ref();
    let _ = listen_fn.call2(&cef, &JsValue::from_str(name), cb);
    closure.forget();
    Ok(())
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
struct PageReadyPayload {}

pub fn try_emit_page_ready() -> Result<(), EventListenerError> {
    use js_sys::{ArrayBuffer, Uint8Array};

    let cef = window_cef()?;
    let emit_fn = cef_bin_emit_fn(&cef)?;

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&PageReadyPayload {})
        .map_err(|_| EventListenerError::SerializePayload)?;
    let envelope = encode_bin_ipc_envelope(PAGE_READY_BIN_EVENT_ID, &bytes);
    let buffer = ArrayBuffer::new(envelope.len() as u32);
    let view = Uint8Array::new(&buffer);
    view.copy_from(&envelope);

    let _ = emit_fn.call1(&cef, &buffer.into());
    Ok(())
}

const LISTENER_RETRY_MS: i32 = 16;

fn schedule_listener_retry(mut retry_tick: Signal<u32>, current: u32) {
    let Some(win) = window() else {
        return;
    };
    let closure = Closure::once(move || {
        retry_tick.set(current.wrapping_add(1));
    });
    let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
        closure.as_ref().unchecked_ref(),
        LISTENER_RETRY_MS,
    );
    closure.forget();
}

pub struct BevyState {
    pub is_loading: Signal<bool>,
    pub error: Signal<Option<String>>,
}

pub fn use_bin_event_listener<T, F>(name: &'static str, on_event: F) -> BevyState
where
    T: rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    F: FnMut(T) + 'static,
{
    let listener = use_hook(|| GuardedListener::new(on_event));
    let listener_guard = listener.guard();
    use_drop(move || listener_guard.deactivate());
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut is_listening = use_signal(|| false);
    let retry_tick = use_signal(|| 0u32);

    use_effect(move || {
        let current_retry = retry_tick();
        if is_listening() {
            return;
        }
        let listener = listener.clone();
        let Some(rt) = Runtime::try_current() else {
            is_loading.set(false);
            error.set(Some(
                "use_bin_event_listener: no Dioxus runtime (internal error)".into(),
            ));
            return;
        };
        let scope = current_scope_id();
        match try_cef_bin_listen::<T, _>(name, move |msg| {
            let listener = listener.clone();
            rt.in_scope(scope, || {
                listener.call(msg);
            });
        }) {
            Ok(()) => {
                is_listening.set(true);
                is_loading.set(false);
                error.set(None);
                match try_emit_page_ready() {
                    Ok(()) => {}
                    Err(e) => error.set(Some(format!("cef.binEmit/emit failed: {e}"))),
                }
            }
            Err(e) => {
                is_loading.set(true);
                error.set(Some(format!("cef.binListen failed: {e}")));
                schedule_listener_retry(retry_tick, current_retry);
            }
        }
    });

    BevyState { is_loading, error }
}
