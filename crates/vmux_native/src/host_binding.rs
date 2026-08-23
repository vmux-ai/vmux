//! The host a page reaches while its components are running.
//!
//! A real page does not only render — it emits, listens, and reads a theme, and every one of those
//! goes through a host the page never names. The host is installed for the thread around each
//! entry into the `VirtualDom`, so a probe has to do the installing that
//! [`WebView`](crate::WebView) does in production, or a real page mounts inert and a test learns
//! nothing.
//!
//! Erased as a closure returning a guard, for the same reason [`Instance`](crate::Instance) is:
//! the host's type is the caller's, and naming it here would drag the whole page runtime into a
//! crate that deliberately knows nothing about it.

use std::any::Any;

/// How to put a page's host in place for as long as the page is running.
///
/// Built from anything that returns a guard — `HostScope::enter(host)` in practice — and entered
/// again around every render and every event, because both can emit.
#[derive(Default)]
pub struct HostBinding(Option<Box<dyn Fn() -> Box<dyn Any>>>);

impl HostBinding {
    /// Bind a host, given a way to install it that yields a guard undoing the install on drop.
    ///
    /// ```ignore
    /// HostBinding::of(move || HostScope::enter(host.clone()))
    /// ```
    pub fn of<G: 'static>(enter: impl Fn() -> G + 'static) -> Self {
        Self(Some(Box::new(move || Box::new(enter()) as Box<dyn Any>)))
    }

    /// Install the host, for as long as the returned value is held.
    ///
    /// `None` is the unbound case and is not an error: a component that never reaches its host
    /// renders the same either way.
    pub(crate) fn entered(&self) -> Option<Box<dyn Any>> {
        let enter = self.0.as_ref()?;

        Some(enter())
    }
}
