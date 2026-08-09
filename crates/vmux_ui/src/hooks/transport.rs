//! Where a page sends its messages, and where it hears back.
//!
//! Pages emit typed rkyv payloads and subscribe by event id. *How* those bytes reach the host
//! differs: the desktop UI is wasm inside a CEF browser and crosses a real process boundary via
//! `window.cef`, while the mobile app runs Rust natively in the same process as its WebView and
//! has no boundary to cross at all. This module is the seam between the two.
//!
//! Hosts install an implementation at startup. On wasm the CEF bridge is assumed when nothing is
//! installed, so the desktop needs no wiring.

use std::cell::RefCell;
use std::rc::Rc;

use crate::hooks::event_listener::EventListenerError;

/// Receives the raw payload bytes of one host event.
pub type BytesListener = Box<dyn FnMut(&[u8])>;

/// A page's channel to whatever is hosting it.
///
/// Ids are passed explicitly rather than derived, because the two directions do not agree: a page
/// emits under `std::any::type_name::<T>()` while the host pushes under short constants like
/// `"theme"`. Keeping the id a parameter lets each implementation honour its own convention.
pub trait PageHost {
    /// Deliver a serialized payload to the host. Fire-and-forget.
    fn emit(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError>;

    /// Register interest in an event id. The callback receives raw payload bytes.
    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError>;
}

thread_local! {
    static HOST: RefCell<Option<Rc<dyn PageHost>>> = const { RefCell::new(None) };
}

/// Install the host for this thread. Call once, before the first page mounts.
pub fn install_host(host: Rc<dyn PageHost>) {
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
}

fn with_host<R>(f: impl FnOnce(&dyn PageHost) -> R) -> Result<R, EventListenerError> {
    let installed = HOST.with(|slot| slot.borrow().clone());
    if let Some(host) = installed {
        return Ok(f(host.as_ref()));
    }
    #[cfg(web)]
    {
        Ok(f(&super::cef_host::CefHost))
    }
    #[cfg(not(web))]
    {
        Err(EventListenerError::NoHost)
    }
}

pub fn emit_bytes(id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
    with_host(|host| host.emit(id, bytes))?
}

pub fn listen_bytes(id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
    with_host(|host| host.listen(id, on_bytes))?
}

/// Decode a host payload. Shared by every transport — only the delivery differs.
pub fn decode_bin_payload<T>(bytes: &[u8]) -> Option<T>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    rkyv::from_bytes::<T, rkyv::rancor::Error>(bytes).ok()
}

#[cfg(test)]
#[path = "transport.test.rs"]
mod tests;
