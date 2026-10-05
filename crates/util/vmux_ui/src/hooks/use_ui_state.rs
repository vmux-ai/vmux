use crate::listener_guard::GuardedListener;
use crate::transport::Host;
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;
use vmux_api::UiState;

struct UiStateListener {
    error: Signal<Option<String>>,
}

pub struct UiStateBinding<T> {
    pub state: Signal<T>,
    pub error: Signal<Option<String>>,
}

fn use_ui_state_listener<T, F>(on_state: F) -> UiStateListener
where
    T: UiState + rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    F: FnMut(T) + 'static,
{
    let listener = use_hook(|| GuardedListener::new(on_state));
    let listener_guard = listener.guard();
    use_drop(move || listener_guard.deactivate());
    let mut error = use_signal(|| None::<String>);
    let mut listening = use_signal(|| false);
    let retry_tick = use_signal(|| 0u32);

    use_effect(move || {
        let current_retry = retry_tick();
        if listening() {
            return;
        }
        let listener = listener.clone();
        let Some(runtime) = Runtime::try_current() else {
            error.set(Some("use_ui_state: no Dioxus runtime".into()));
            return;
        };
        let scope = current_scope_id();
        match Host::listen_state::<T, _>(move |state| {
            let listener = listener.clone();
            runtime.in_scope(scope, || listener.call(state));
        }) {
            Ok(()) => {
                listening.set(true);
                error.set(None);
                if let Err(cause) = Host::announce_ready() {
                    error.set(Some(format!("page ready emit failed: {cause}")));
                }
            }
            Err(cause) => {
                error.set(Some(format!("host listen failed: {cause}")));
                Host::schedule_listener_retry(retry_tick, current_retry);
            }
        }
    });

    UiStateListener { error }
}

impl<T> Clone for UiStateBinding<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for UiStateBinding<T> {}

pub fn use_ui_state<T>() -> UiStateBinding<T>
where
    T: UiState + rkyv::Archive + Default + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let mut state = use_signal(T::default);
    let listener = use_ui_state_listener::<T, _>(move |event| state.set(event));
    let binding = UiStateBinding {
        state,
        error: listener.error,
    };
    use_context_provider(|| binding);
    binding
}
