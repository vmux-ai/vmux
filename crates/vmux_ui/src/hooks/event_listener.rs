//! The host bridge: encoding a payload, handing it to whatever transport is installed, and
//! naming every way that can fail.
//!
//! No hooks live here — see the `use_*` modules beside it for those.

use std::fmt;

use crate::hooks::transport::{decode_bin_payload, emit_bytes, listen_bytes};

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
    /// The installed host has no route for that event id. Unlike the other variants this is not a
    /// fault: a host that can only serve part of a page says so rather than silently succeeding.
    Unsupported,
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
            Self::Unsupported => "the host has no route for this event",
        })
    }
}

/// Send a typed payload to the host.
///
/// The event id is the payload's type name, which is what the Bevy side matches on.
pub fn send<T>(payload: &T) -> Result<(), EventListenerError>
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
