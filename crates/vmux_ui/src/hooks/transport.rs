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
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    struct Ping {
        value: u32,
    }

    #[derive(Default)]
    struct LoopbackHost {
        listeners: RefCell<Vec<(String, BytesListener)>>,
    }

    impl PageHost for LoopbackHost {
        fn emit(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            for (registered, on_bytes) in self.listeners.borrow_mut().iter_mut() {
                if registered == id {
                    on_bytes(bytes);
                }
            }
            Ok(())
        }

        fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
            self.listeners.borrow_mut().push((id.to_string(), on_bytes));
            Ok(())
        }
    }

    #[test]
    fn a_payload_reaches_the_listener_registered_for_its_id() {
        install_host(Rc::new(LoopbackHost::default()));

        let seen = Rc::new(RefCell::new(Vec::<Ping>::new()));
        let sink = seen.clone();
        listen_bytes(
            "ping",
            Box::new(move |bytes| {
                if let Some(ping) = decode_bin_payload::<Ping>(bytes) {
                    sink.borrow_mut().push(ping);
                }
            }),
        )
        .unwrap();

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&Ping { value: 7 }).unwrap();
        emit_bytes("ping", &bytes).unwrap();
        emit_bytes("other", &bytes).unwrap();

        assert_eq!(*seen.borrow(), vec![Ping { value: 7 }]);
    }
}
