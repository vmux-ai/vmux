//! The other side of a page: what is hosting it, and how bytes reach it.
//!
//! Nothing here is a hook. Pages emit typed rkyv payloads and subscribe by event id; *how* those
//! bytes travel differs. The desktop UI is wasm inside a CEF browser and crosses a real process
//! boundary via `window.cef`, while the mobile app runs Rust natively in the same process as its
//! WebView and has no boundary to cross at all. [`PageHost`] is that seam at runtime — an app
//! installs one and every message travels over it — and [`Host`] is the same difference resolved
//! at compile time rather than tested for at every call site. The hooks in [`crate::hooks`] are
//! built on top of both and carry no target test of their own.
//!
//! Hosts install an implementation at startup. On wasm the CEF bridge is assumed when nothing is
//! installed, so the desktop needs no wiring.
//!
//! [`event_listener`] types what a page sends and names every way sending can fail, and
//! [`bin_ipc_envelope`] is the framing the CEF direction adds on the way out.

use std::cell::RefCell;
use std::rc::Rc;

use crate::transport::event_listener::EventListenerError;

/// A page's channel to whatever is hosting it.
///
/// Ids are passed explicitly rather than derived, because the two directions do not agree: a page
/// emits under `std::any::type_name::<T>()` while the host pushes under short constants like
/// `"theme"`. Keeping the id a parameter lets each implementation honour its own convention.
pub trait PageHost {
    /// Deliver a serialized payload to the host. Fire-and-forget.
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError>;

    /// Register interest in an event id. The callback receives raw payload bytes.
    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError>;
}

/// Receives the raw payload bytes of one host event.
pub type BytesListener = Box<dyn FnMut(&[u8])>;

/// Install the host for this thread. Call once, before the first page mounts.
///
/// An embedding app calls this at startup the way a runtime calls `main` — before there is a page,
/// and so before there is a [`Host`] for it to hang off.
pub fn install_host(host: Rc<dyn PageHost>) {
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
}

/// What the target hosting a page can do for it, decided at compile time.
///
/// Every capability is implemented once per target in a submodule — exactly one of which is
/// compiled. Distinct from [`PageHost`], which an app installs at runtime and which two builds for
/// the same target may answer differently; this one *is* the target.
pub(crate) struct Host;

impl Host {
    pub(crate) fn emit(id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        Self::with_installed(|host| host.send(id, bytes))?
    }

    pub(crate) fn listen(id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        Self::with_installed(|host| host.listen(id, on_bytes))?
    }

    /// The host an app installed, or the one this target assumes when nobody did.
    fn with_installed<R>(f: impl FnOnce(&dyn PageHost) -> R) -> Result<R, EventListenerError> {
        let installed = HOST.with(|slot| slot.borrow().clone());
        if let Some(host) = installed {
            return Ok(f(host.as_ref()));
        }
        let Some(fallback) = Self::fallback() else {
            return Err(EventListenerError::NoHost);
        };
        Ok(f(fallback))
    }
}

/// The bytes of one host event, before anything has read a type into them.
///
/// Every transport delivers the same thing and only the delivery differs, so decoding hangs off
/// the payload rather than off whichever bridge happened to carry it.
pub struct HostPayload<'a>(&'a [u8]);

impl<'a> HostPayload<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    /// Read the payload as `T`, or `None` if the bytes are not a valid archived `T`.
    pub fn decode<T>(&self) -> Option<T>
    where
        T: rkyv::Archive,
        T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
            + for<'b> rkyv::bytecheck::CheckBytes<
                rkyv::api::high::HighValidator<'b, rkyv::rancor::Error>,
            >,
    {
        rkyv::from_bytes::<T, rkyv::rancor::Error>(self.0).ok()
    }
}

thread_local! {
    static HOST: RefCell<Option<Rc<dyn PageHost>>> = const { RefCell::new(None) };
}

pub mod bin_ipc_envelope;
pub mod event_listener;
#[cfg(not(web))]
mod native;
#[cfg(web)]
pub mod web;

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
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
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
        Host::listen(
            "ping",
            Box::new(move |bytes| {
                if let Some(ping) = HostPayload::new(bytes).decode::<Ping>() {
                    sink.borrow_mut().push(ping);
                }
            }),
        )
        .unwrap();

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&Ping { value: 7 }).unwrap();
        Host::emit("ping", &bytes).unwrap();
        Host::emit("other", &bytes).unwrap();

        assert_eq!(*seen.borrow(), vec![Ping { value: 7 }]);
    }
}
