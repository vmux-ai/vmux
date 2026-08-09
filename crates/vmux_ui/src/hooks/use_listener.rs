//! Subscribing a page to one typed host event.

use crate::hooks::Host;
use crate::hooks::event_listener::{try_cef_bin_listen, try_emit_page_ready};
use crate::listener_guard::GuardedListener;
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;

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
                Host::schedule_listener_retry(retry_tick, current_retry);
            }
        }
    });

    BevyState { is_loading, error }
}
