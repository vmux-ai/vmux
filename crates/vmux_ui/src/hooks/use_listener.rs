//! Subscribing a page to one typed host event.

use crate::hooks::event_listener::{try_cef_bin_listen, try_emit_page_ready};
use crate::listener_guard::GuardedListener;
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;

/// Retry until the CEF bridge is injected.
///
/// Only wasm needs this: the bridge appears asynchronously after the page loads, whereas a native
/// host installs its transport before the first page mounts.
#[cfg(web)]
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

#[cfg(not(web))]
fn schedule_listener_retry(_retry_tick: Signal<u32>, _current: u32) {}

pub struct BevyState {
    pub is_loading: Signal<bool>,
    pub error: Signal<Option<String>>,
}

pub fn use_listener<T, F>(name: &'static str, on_event: F) -> BevyState
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
                "use_listener: no Dioxus runtime (internal error)".into(),
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
