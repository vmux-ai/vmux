use std::fmt;

use crate::hooks::transport::{decode_bin_payload, emit_bytes, listen_bytes};
use crate::listener_guard::GuardedListener;
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;

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
    /// No [`crate::hooks::transport::PageHost`] installed on a target with no default.
    NoHost,
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
            Self::NoHost => "no page host installed",
        })
    }
}

/// Decode a host payload out of a raw JS value.
#[cfg(target_arch = "wasm32")]
pub fn decode_bin_host_emit_js<T>(e: &wasm_bindgen::JsValue) -> Option<T>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    crate::hooks::cef_host::js_value_bytes(e).and_then(|bytes| decode_bin_payload::<T>(&bytes))
}

/// Send a typed payload to the host.
///
/// The event id is the payload's type name, which is what the Bevy side matches on.
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
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(payload)
        .map_err(|_| EventListenerError::SerializePayload)?;
    emit_bytes(std::any::type_name::<T>(), &bytes)
}

pub fn try_cef_bin_listen<T, F>(name: &str, on_event: F) -> Result<(), EventListenerError>
where
    T: rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    F: FnMut(T) + 'static,
{
    let mut on_event = on_event;
    listen_bytes(
        name,
        Box::new(move |bytes| {
            if let Some(msg) = decode_bin_payload::<T>(bytes) {
                on_event(msg);
            }
        }),
    )
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
struct PageReadyPayload {}

pub fn try_emit_page_ready() -> Result<(), EventListenerError> {
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&PageReadyPayload {})
        .map_err(|_| EventListenerError::SerializePayload)?;
    emit_bytes(PAGE_READY_BIN_EVENT_ID, &bytes)
}

/// Retry until the CEF bridge is injected.
///
/// Only wasm needs this: the bridge appears asynchronously after the page loads, whereas a native
/// host installs its transport before the first page mounts.
#[cfg(target_arch = "wasm32")]
fn schedule_listener_retry(mut retry_tick: Signal<u32>, current: u32) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    const LISTENER_RETRY_MS: i32 = 16;

    let Some(win) = web_sys::window() else {
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

#[cfg(not(target_arch = "wasm32"))]
fn schedule_listener_retry(_retry_tick: Signal<u32>, _current: u32) {}

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
                    Err(e) => error.set(Some(format!("page ready emit failed: {e}"))),
                }
            }
            Err(e) => {
                is_loading.set(true);
                error.set(Some(format!("host listen failed: {e}")));
                schedule_listener_retry(retry_tick, current_retry);
            }
        }
    });

    BevyState { is_loading, error }
}

/// Maps the latest binary host event into a Dioxus signal.
pub fn use_event<T>(name: &'static str, init: impl FnOnce() -> T) -> Signal<T>
where
    T: rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let mut state = use_signal(init);
    let _listener = use_bin_event_listener::<T, _>(name, move |event| state.set(event));
    state
}
