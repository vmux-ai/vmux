//! Dioxus hooks connecting a page to whatever hosts it.
//!
//! One hook per module, named after it. [`transport`] is the seam underneath them: the desktop
//! reaches Bevy across a CEF process boundary, a native host handles messages in-process. Pages
//! call the same hooks either way.
//!
//! Which frontend a page is compiled for decides the rest — whether there is a default transport,
//! whether a failed subscription is worth retrying, and whether there is a document to write to.
//! `Host` names those, implemented once in `cef_host` and once in `native_host`, so the hooks
//! themselves carry no target test.
//!
//! [`event_listener`] and [`list_nav`] hold no hooks — they are the host bridge and the pure
//! keyboard logic that the hooks here are built on.

#[cfg(web)]
pub mod cef_host;
pub mod event_listener;
pub mod list_nav;
#[cfg(not(web))]
mod native_host;
pub mod transport;
mod use_event;
mod use_listener;
#[cfg(web)]
mod use_mobile;
mod use_selector;
mod use_theme;

/// What the frontend hosting a page can do for it, decided at compile time.
///
/// Every capability is implemented once per frontend in a sibling module — exactly one of which is
/// compiled. Distinct from [`transport::PageHost`], which an app installs at runtime and which two
/// builds of the same frontend may answer differently; this one *is* the target.
pub(crate) struct Host;

#[allow(unused_imports)]
pub use event_listener::{EventListenerError, send, try_cef_bin_listen, try_emit_page_ready};

#[cfg(web)]
pub use cef_host::decode_bin_host_emit_js;

pub use list_nav::{
    ListKey, MenuDirection, choice_number_index, list_key, menu_direction, move_selection,
};
pub use transport::{PageHost, install_host};
pub use use_event::use_event;
pub use use_listener::{BevyState, use_listener};
#[cfg(web)]
pub use use_mobile::use_mobile;
pub use use_selector::use_selector;
pub use use_theme::use_theme;
