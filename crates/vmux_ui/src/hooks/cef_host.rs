//! The CEF bridge: how a wasm page reaches the Bevy process.
//!
//! The framing is asymmetric and load-bearing. Page→host sends one buffer carrying a
//! `vmux-bin-ipc-v1` envelope whose id the Bevy side matches with `bin_ipc_event_id::<E>()`;
//! host→page arrives as a bare `ArrayBuffer` under a short string id. Both are preserved exactly.

use js_sys::Function;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::window;

use crate::bin_ipc_envelope::encode_bin_ipc_envelope;
use crate::hooks::event_listener::EventListenerError;
use crate::hooks::transport::{BytesListener, PageHost};

pub struct CefHost;

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

fn cef_bin_emit_fn(cef: &JsValue) -> Result<Function, EventListenerError> {
    let Ok(emit) = js_sys::Reflect::get(cef, &JsValue::from_str("binEmit")) else {
        return Err(EventListenerError::NoEmitMethod);
    };
    emit.dyn_into::<Function>()
        .map_err(|_| EventListenerError::EmitNotCallable)
}

fn cef_bin_listen_fn(cef: &JsValue) -> Result<Function, EventListenerError> {
    let Ok(listen) = js_sys::Reflect::get(cef, &JsValue::from_str("binListen")) else {
        return Err(EventListenerError::NoListenMethod);
    };
    listen
        .dyn_into::<Function>()
        .map_err(|_| EventListenerError::ListenNotCallable)
}

/// Copy an `ArrayBuffer` or `Uint8Array` out of JS.
pub fn js_value_bytes(value: &JsValue) -> Option<Vec<u8>> {
    use js_sys::{ArrayBuffer, Uint8Array};

    let buffer: ArrayBuffer = if let Some(buf) = value.dyn_ref::<ArrayBuffer>() {
        buf.clone()
    } else if let Some(arr) = value.dyn_ref::<Uint8Array>() {
        arr.buffer()
    } else {
        return None;
    };
    let view = Uint8Array::new(&buffer);
    let mut bytes = vec![0u8; view.length() as usize];
    view.copy_to(&mut bytes);
    Some(bytes)
}

impl PageHost for CefHost {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        use js_sys::{ArrayBuffer, Uint8Array};

        let cef = window_cef()?;
        let emit_fn = cef_bin_emit_fn(&cef)?;

        let envelope = encode_bin_ipc_envelope(id, bytes);
        let buffer = ArrayBuffer::new(envelope.len() as u32);
        let view = Uint8Array::new(&buffer);
        view.copy_from(&envelope);

        let _ = emit_fn.call1(&cef, &buffer.into());
        Ok(())
    }

    fn listen(&self, id: &str, mut on_bytes: BytesListener) -> Result<(), EventListenerError> {
        let cef = window_cef()?;
        let listen_fn = cef_bin_listen_fn(&cef)?;

        let closure = Closure::wrap(Box::new(move |e: JsValue| {
            if let Some(bytes) = js_value_bytes(&e) {
                on_bytes(&bytes);
            }
        }) as Box<dyn FnMut(JsValue)>);

        let cb = closure.as_ref().unchecked_ref();
        let _ = listen_fn.call2(&cef, &JsValue::from_str(id), cb);
        closure.forget();
        Ok(())
    }
}
