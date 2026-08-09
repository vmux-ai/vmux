//! Dioxus hooks connecting a page to whatever hosts it.
//!
//! One hook per module, named after it. [`transport`] is the seam underneath them: the desktop
//! reaches Bevy across a CEF process boundary, a native host handles messages in-process. Pages
//! call the same hooks either way.
//!
//! [`event_listener`] and [`list_nav`] hold no hooks — they are the host bridge and the pure
//! keyboard logic that the hooks here are built on.

#[cfg(web)]
pub mod cef_host;
pub mod event_listener;
pub mod list_nav;
pub mod transport;
mod use_event;
mod use_listener;
#[cfg(web)]
mod use_mobile;
mod use_selector;
mod use_theme;

#[allow(unused_imports)]
pub use event_listener::{EventListenerError, emit, try_cef_bin_listen, try_emit_page_ready};

#[cfg(web)]
pub use event_listener::decode_bin_host_emit_js;

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
