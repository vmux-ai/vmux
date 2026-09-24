use std::fmt;

use crate::transport::Host;
use crate::transport::HostPayload;
use vmux_api::{PageReady, UiEvent, UiState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventListenerError {
    NoWindow,
    NoHostBridge,
    NoListenMethod,
    ListenNotCallable,
    NoEmitMethod,
    EmitNotCallable,
    SerializePayload,
    NoHost,
    Unsupported,
}

impl fmt::Display for EventListenerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoWindow => "no `window`",
            Self::NoHostBridge => "neither `window.cef` nor `window.vmuxWry` is injected",
            Self::NoListenMethod => "no `binListen` on the host bridge",
            Self::ListenNotCallable => "`binListen` is not a function",
            Self::NoEmitMethod => "no `binEmit` on the host bridge",
            Self::EmitNotCallable => "`binEmit` is not a function",
            Self::SerializePayload => "failed to serialize emit payload",
            Self::NoHost => "no page host installed",
            Self::Unsupported => "the host has no route for this event",
        })
    }
}

pub fn send<T>(payload: &T) -> Result<(), EventListenerError>
where
    T: UiEvent
        + for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
{
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(payload)
        .map_err(|_| EventListenerError::SerializePayload)?;
    Host::emit(T::TARGET, T::id(), &bytes)
}

pub(crate) fn listen_ui_state<T, F>(on_event: F) -> Result<(), EventListenerError>
where
    T: UiState + rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    F: FnMut(T) + 'static,
{
    let mut on_event = on_event;
    Host::listen(
        T::TARGET,
        T::id(),
        Box::new(move |bytes| {
            if let Some(msg) = HostPayload::new(bytes).decode::<T>() {
                on_event(msg);
            }
        }),
    )
}

pub(crate) fn try_emit_page_ready() -> Result<(), EventListenerError> {
    send(&PageReady)
}
