//! The latest host event, as a signal.

use crate::hooks::use_listener::use_listener;
use dioxus::prelude::*;

/// Maps the latest binary host event into a Dioxus signal.
pub fn use_event<T>(name: &'static str, init: impl FnOnce() -> T) -> Signal<T>
where
    T: rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let mut state = use_signal(init);
    let _listener = use_listener::<T, _>(name, move |event| state.set(event));
    state
}
