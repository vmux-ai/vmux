use crate::listener_guard::GuardedListener;
use crate::transport::Host;
use crate::transport::event_listener::{EventListenerError, listen_ui_state, try_emit_page_ready};
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

struct UiStateListener {
    error: Signal<Option<String>>,
}

#[derive(Clone)]
struct PageReadyAnnouncement(Rc<Cell<bool>>);

impl PageReadyAnnouncement {
    fn new() -> Self {
        use_root_context(|| Self(Rc::new(Cell::new(false))))
    }

    fn announce(&self) -> Result<(), EventListenerError> {
        if self.0.get() {
            return Ok(());
        }
        try_emit_page_ready()?;
        self.0.set(true);
        Ok(())
    }
}

pub struct UiStatePatchBatch<S, T> {
    state: Signal<S>,
    handled_sequence: Signal<u64>,
    payload: PhantomData<fn() -> T>,
}

pub struct UiStateRoot<T> {
    pub state: Signal<T>,
    pub error: Signal<Option<String>>,
}

fn use_ui_state_listener<T, F>(on_state: F) -> UiStateListener
where
    T: vmux_api::UiState + rkyv::Archive + 'static,
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
    let announcement = PageReadyAnnouncement::new();

    use_effect(move || {
        let current_retry = retry_tick();
        if listening() {
            return;
        }
        let announcement = announcement.clone();
        let listener = listener.clone();
        let Some(runtime) = Runtime::try_current() else {
            error.set(Some("use_ui_state: no Dioxus runtime".into()));
            return;
        };
        let scope = current_scope_id();
        match listen_ui_state::<T, _>(move |state| {
            let listener = listener.clone();
            runtime.in_scope(scope, || listener.call(state));
        }) {
            Ok(()) => {
                listening.set(true);
                error.set(None);
                if let Err(cause) = announcement.announce() {
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

impl<T> Clone for UiStateRoot<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for UiStateRoot<T> {}

impl<S, T> Clone for UiStatePatchBatch<S, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, T> Copy for UiStatePatchBatch<S, T> {}

impl<S, T> UiStatePatchBatch<S, T>
where
    S: vmux_api::BatchedUiState,
    S::Patch: vmux_api::UiStatePatch<T>,
    T: Clone + 'static,
{
    pub fn take(mut self) -> Vec<T> {
        let (sequence, payloads) = {
            let event = self.state.read();
            let sequence = event.sequence();
            if sequence == 0 || sequence == *self.handled_sequence.peek() {
                return Vec::new();
            }
            let payloads = event
                .patches()
                .iter()
                .filter_map(<S::Patch as vmux_api::UiStatePatch<T>>::payload)
                .cloned()
                .collect();
            (sequence, payloads)
        };
        self.handled_sequence.set(sequence);
        payloads
    }

    pub fn for_each(self, mut callback: impl FnMut(T)) {
        for payload in self.take() {
            callback(payload);
        }
    }
}

pub fn use_ui_state<T>() -> Signal<T>
where
    T: vmux_api::UiState + rkyv::Archive + Default + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let mut state = use_signal(T::default);
    let _listener = use_ui_state_listener::<T, _>(move |event| state.set(event));
    state
}

pub fn use_ui_state_root<T>() -> UiStateRoot<T>
where
    T: vmux_api::BatchedUiState + rkyv::Archive + Default + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let mut state = use_signal(T::default);
    let listener = use_ui_state_listener::<T, _>(move |event| state.set(event));
    use_context_provider(|| state);
    UiStateRoot {
        state,
        error: listener.error,
    }
}

pub fn use_ui_state_patch<S, T>() -> UiStatePatchBatch<S, T>
where
    S: vmux_api::BatchedUiState,
    S::Patch: vmux_api::UiStatePatch<T>,
    T: Clone + 'static,
{
    let state = use_context::<Signal<S>>();
    UiStatePatchBatch {
        state,
        handled_sequence: use_signal(|| 0),
        payload: PhantomData,
    }
}
