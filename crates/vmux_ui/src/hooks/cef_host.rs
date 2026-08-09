//! The wasm frontend: a page inside a CEF browser, reaching the Bevy process.
//!
//! The framing is asymmetric and load-bearing. Page→host sends one buffer carrying a
//! `vmux-bin-ipc-v1` envelope whose id the Bevy side matches with `bin_ipc_event_id::<E>()`;
//! host→page arrives as a bare `ArrayBuffer` under a short string id. Both are preserved exactly.
//!
//! This is also the frontend that has a real document, so the parts of theming and scrolling that
//! reach for one are answered here.

use dioxus::prelude::*;
use js_sys::Function;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::window;

use crate::bin_ipc_envelope::encode_bin_ipc_envelope;
use crate::hooks::Host;
use crate::hooks::event_listener::EventListenerError;
use crate::hooks::transport::{BytesListener, PageHost, decode_bin_payload};

impl Host {
    /// The CEF bridge, assumed when no host installs one — which is every desktop page.
    pub(crate) fn fallback() -> Option<&'static dyn PageHost> {
        Some(&CefHost)
    }

    /// Retry until the CEF bridge is injected.
    ///
    /// Only wasm needs this: the bridge appears asynchronously after the page loads, whereas a
    /// native host installs its transport before the first page mounts.
    pub(crate) fn schedule_listener_retry(mut retry_tick: Signal<u32>, current: u32) {
        const LISTENER_RETRY_MS: i32 = 16;

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

    pub(crate) fn scroll_item_into_view(item_id: &str) {
        let Some(element) = window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(item_id))
        else {
            return;
        };
        let options = web_sys::ScrollIntoViewOptions::new();
        options.set_block(web_sys::ScrollLogicalPosition::Nearest);
        element.scroll_into_view_with_scroll_into_view_options(&options);
    }

    pub(crate) fn set_root_radius(radius: f32) {
        let Some(el) = window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element())
        else {
            return;
        };
        let html: &web_sys::HtmlElement = el.unchecked_ref();
        let _ = html
            .style()
            .set_property("--radius", &format!("{radius}px"));
    }

    pub(crate) fn set_root_language(locale: &str, direction: &str) {
        let Some(el) = window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element())
        else {
            return;
        };
        let _ = el.set_attribute("lang", locale);
        let _ = el.set_attribute("dir", direction);
    }
}

pub struct CefHost;

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

/// Decode a host payload out of a raw JS value.
pub fn decode_bin_host_emit_js<T>(e: &wasm_bindgen::JsValue) -> Option<T>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    js_value_bytes(e).and_then(|bytes| decode_bin_payload::<T>(&bytes))
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
