use crate::hooks::use_listener::use_listener;
use dioxus::prelude::*;

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

pub fn use_ui_state_patch<S, T>() -> ReadSignal<Option<T>>
where
    S: vmux_api::UiState,
    S::Patch: vmux_api::UiStatePatch<T>,
    T: Clone + 'static,
{
    let state = use_context::<Signal<S>>();
    let mut value = use_signal(|| None);
    let mut handled_sequence = use_signal(|| 0u64);
    use_effect(move || {
        let event = state();
        if event.sequence() == 0 || event.sequence() == *handled_sequence.peek() {
            return;
        }
        handled_sequence.set(event.sequence());
        let Some(payload) = event
            .patches()
            .iter()
            .rev()
            .find_map(<S::Patch as vmux_api::UiStatePatch<T>>::payload)
        else {
            return;
        };
        value.set(Some(payload.clone()));
    });
    value.into()
}

pub fn use_ui_state_events<S, T, F>(mut on_event: F)
where
    S: vmux_api::UiState,
    S::Patch: vmux_api::UiStatePatch<T>,
    T: Clone + 'static,
    F: FnMut(T) + 'static,
{
    let state = use_context::<Signal<S>>();
    let mut handled_sequence = use_signal(|| 0u64);
    use_effect(move || {
        let event = state();
        if event.sequence() == 0 || event.sequence() == *handled_sequence.peek() {
            return;
        }
        handled_sequence.set(event.sequence());
        for patch in event.patches() {
            let Some(payload) = <S::Patch as vmux_api::UiStatePatch<T>>::payload(patch) else {
                continue;
            };
            on_event(payload.clone());
        }
    });
}
