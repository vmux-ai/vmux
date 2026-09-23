use crate::hooks::use_listener::use_listener;
use dioxus::prelude::*;
use std::marker::PhantomData;

pub struct UiStatePatchBatch<S, T> {
    state: Signal<S>,
    handled_sequence: Signal<u64>,
    payload: PhantomData<fn() -> T>,
}

impl<S, T> Clone for UiStatePatchBatch<S, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, T> Copy for UiStatePatchBatch<S, T> {}

impl<S, T> UiStatePatchBatch<S, T>
where
    S: vmux_api::UiState,
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

    pub fn for_each(self, mut action: impl FnMut(T)) {
        for payload in self.take() {
            action(payload);
        }
    }
}

pub fn use_ui_state<T>() -> Signal<T>
where
    T: vmux_api::HostEvent + rkyv::Archive + Default + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let mut state = use_signal(T::default);
    let _listener = use_listener::<T, _>(move |event| state.set(event));
    state
}

pub fn use_ui_state_root<T>() -> Signal<T>
where
    T: vmux_api::HostEvent + vmux_api::UiState + rkyv::Archive + Default + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let state = use_ui_state::<T>();
    use_context_provider(|| state);
    state
}

pub fn use_ui_state_patch<S, T>() -> UiStatePatchBatch<S, T>
where
    S: vmux_api::UiState,
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
