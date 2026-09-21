use crate::hooks::use_listener::use_listener;
use dioxus::prelude::*;

pub fn use_event<T>(init: impl FnOnce() -> T) -> Signal<T>
where
    T: vmux_api::HostEvent + rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let mut state = use_signal(init);
    let _listener = use_listener::<T, _>(move |event| state.set(event));
    state
}
