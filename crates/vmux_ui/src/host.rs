//! The other side of a page: what is hosting it, and how bytes reach it.
//!
//! Nothing here is a hook. A page is served either by a CEF browser talking to the Bevy process
//! or by a native WebView with Rust in the same process, and [`Host`] is that difference resolved
//! at compile time rather than tested for at every call site. The hooks in [`crate::hooks`] are
//! built on top of it and carry no target test of their own.
//!
//! [`transport`] is the runtime half — an app installs a [`transport::PageHost`] and every message
//! travels over it. [`event_listener`] types what a page sends and names every way sending can
//! fail, and [`bin_ipc_envelope`] is the framing the CEF direction adds on the way out.

/// What the frontend hosting a page can do for it, decided at compile time.
///
/// Every capability is implemented once per frontend in a sibling module — exactly one of which is
/// compiled. Distinct from [`transport::PageHost`], which an app installs at runtime and which two
/// builds of the same frontend may answer differently; this one *is* the target.
pub(crate) struct Host;

pub mod bin_ipc_envelope;
#[cfg(web)]
pub mod cef;
pub mod event_listener;
#[cfg(not(web))]
mod native;
pub mod transport;
