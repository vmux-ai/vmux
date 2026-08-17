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

    /// Give keyboard focus to an element the page rendered.
    ///
    /// A capability of the host rather than of the target, because whether there is a document to
    /// focus into is a property of what is hosting the page: wasm reaches it directly, the phone
    /// has no separate document, and the desktop has one it can only reach by evaluating a script.
    /// The default is the phone's answer.
    fn focus_element(&self, _element_id: &str) {}

    /// Scroll the least that brings an element the page rendered into view.
    ///
    /// A host capability for the same reason as [`Self::focus_element`]: there is a viewport to
    /// scroll only where something hosts the page in a document. The default is the phone's answer,
    /// which has no keyboard to move a selection out of view in the first place.
    fn scroll_element_into_view(&self, _element_id: &str) {}

    /// Highlight the whole value of a text field, leaving the view where it is.
    fn select_element_text(&self, _element_id: &str) {}

    /// Focus a text field and offer its value up to be overtyped: selected whole, and rewound to
    /// the start so a long one reads as an offer rather than as a tail.
    fn offer_element_text(&self, _element_id: &str) {}

    /// Where the caret sits in a text field, as a UTF-8 byte offset into its value.
    ///
    /// The one capability here that needs an answer rather than an instruction, which is why a
    /// host may only be able to give a remembered one. Zero when it has nothing to report.
    fn caret_position(&self, _element_id: &str) -> usize {
        0
    }

    /// Put the caret at a UTF-8 byte offset into a text field's value.
    fn place_caret(&self, _element_id: &str, _byte: usize) {}
}

/// Receives the raw payload bytes of one host event.
pub type BytesListener = Box<dyn FnMut(&[u8])>;

/// Install the host for this thread. Call once, before the first page mounts.
///
/// An embedding app calls this at startup the way a runtime calls `main` — before there is a page,
/// and so before there is a [`Host`] for it to hang off.
///
/// One host per thread, permanently. A thread running more than one page wants [`HostScope`]
/// instead — this would leave every page but the last talking to the wrong host.
pub fn install_host(host: Rc<dyn PageHost>) {
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
}

/// The installed host, for as long as this value lives.
///
/// A page reaches its host through a thread-local, which is what lets a component deep in a tree
/// emit without being handed anything. That is exactly right for one page per thread and wrong the
/// moment there are two: whichever mounted last would own the slot, and every other page's `send`
/// and `listen` would silently address it.
///
/// So a host that runs several pages on one thread does not install one — it enters the scope of
/// the page it is about to touch, around every entry into that page's `VirtualDom`. Restoring the
/// previous value rather than clearing it is what makes those entries nest safely, which they do:
/// a listener runs inside `deliver`, and an event handler can emit.
pub struct HostScope(Option<Rc<dyn PageHost>>);

impl HostScope {
    pub fn enter(host: Rc<dyn PageHost>) -> Self {
        Self(HOST.with(|slot| slot.borrow_mut().replace(host)))
    }
}

impl Drop for HostScope {
    fn drop(&mut self) {
        HOST.with(|slot| *slot.borrow_mut() = self.0.take());
    }
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

    /// `not(web)` only: a page compiled for the browser reaches the document itself, so its
    /// `FocusClaim` never asks the host.
    #[cfg(not(web))]
    pub(crate) fn focus_element(id: &str) {
        let _ = Self::with_installed(|host| host.focus_element(id));
    }

    /// `not(web)` only, for the same reason as [`Self::focus_element`]: on the web
    /// [`crate::caret::TextCaret`] reaches the field itself.
    #[cfg(not(web))]
    pub(crate) fn select_element_text(id: &str) {
        let _ = Self::with_installed(|host| host.select_element_text(id));
    }

    /// `not(web)` only. See [`Self::select_element_text`].
    #[cfg(not(web))]
    pub(crate) fn offer_element_text(id: &str) {
        let _ = Self::with_installed(|host| host.offer_element_text(id));
    }

    /// `not(web)` only. See [`Self::select_element_text`].
    #[cfg(not(web))]
    pub(crate) fn caret_position(id: &str) -> usize {
        Self::with_installed(|host| host.caret_position(id)).unwrap_or(0)
    }

    /// `not(web)` only. See [`Self::select_element_text`].
    #[cfg(not(web))]
    pub(crate) fn place_caret(id: &str, byte: usize) {
        let _ = Self::with_installed(|host| host.place_caret(id, byte));
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

    /// Counts what was sent to it, which is the only way to tell two hosts apart from outside.
    #[derive(Default)]
    struct CountingHost {
        sent: RefCell<usize>,
    }

    impl PageHost for CountingHost {
        fn send(&self, _id: &str, _bytes: &[u8]) -> Result<(), EventListenerError> {
            *self.sent.borrow_mut() += 1;
            Ok(())
        }

        fn listen(&self, _id: &str, _on_bytes: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    /// Two pages on one thread each reach their own host, and a page nested inside another's
    /// delivery does not leave the outer one addressing the wrong host afterwards.
    ///
    /// Without a scope there is one slot: the second page to mount would own it, and everything the
    /// first sent from then on would arrive at the second's entity — no error, no warning.
    #[test]
    fn a_page_addresses_its_own_host_even_while_another_is_mounted() {
        let first = Rc::new(CountingHost::default());
        let second = Rc::new(CountingHost::default());

        let outer = HostScope::enter(first.clone());
        Host::emit("a", &[]).unwrap();
        {
            let _inner = HostScope::enter(second.clone());
            Host::emit("b", &[]).unwrap();
            Host::emit("c", &[]).unwrap();
        }
        Host::emit("d", &[]).unwrap();
        drop(outer);

        assert_eq!(*first.sent.borrow(), 2, "the outer page sent 'a' and 'd'");
        assert_eq!(*second.sent.borrow(), 2, "the inner page sent 'b' and 'c'");
        assert!(
            matches!(Host::emit("e", &[]), Err(EventListenerError::NoHost)),
            "leaving the last scope leaves no host installed, rather than the first one"
        );
    }
}
