//! Dioxus hooks connecting a page to whatever hosts it.
//!
//! [`transport`] is the seam: the desktop reaches Bevy across a CEF process boundary, a native
//! host handles messages in-process. Pages call the same hooks either way.

#[cfg(target_arch = "wasm32")]
pub mod cef_host;
pub mod event_listener;
pub mod transport;
mod use_theme;

#[allow(unused_imports)]
pub use event_listener::{
    BevyState, EventListenerError, try_cef_bin_emit_rkyv, try_cef_bin_listen, try_emit_page_ready,
    use_bin_event_listener, use_event,
};

#[cfg(target_arch = "wasm32")]
pub use event_listener::decode_bin_host_emit_js;

pub use transport::{PageHost, install_host};
pub use use_theme::use_theme;
