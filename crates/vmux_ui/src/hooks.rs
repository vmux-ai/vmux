//! Dioxus hooks connecting a page to whatever hosts it.
//!
//! One hook per module, named after it, and nothing else — the platform seam they are built on is
//! [`crate::transport`], the keystroke encoder is `crate::key_stroke` and the pure keyboard logic is
//! [`crate::list_nav`]. Pages call the same hooks whichever frontend they are compiled for.
//!
//! The re-exports below keep `vmux_ui::hooks::*` resolving for pages that import the seam through
//! here; new code should reach for the owning module.

mod use_event;
#[cfg(ui)]
mod use_key_claim;
mod use_listener;
#[cfg(web)]
mod use_mobile;
mod use_selector;
mod use_theme;

pub use use_event::use_event;
#[cfg(ui)]
pub use use_key_claim::{KeyClaim, use_key_claim};
pub use use_listener::{BevyState, use_listener};
#[cfg(web)]
pub use use_mobile::use_mobile;
pub use use_selector::use_selector;
pub use use_theme::use_theme;

pub use crate::transport;
#[allow(unused_imports)]
pub use crate::transport::event_listener::{
    EventListenerError, send, try_cef_bin_listen, try_emit_page_ready,
};

#[cfg(web)]
pub use crate::transport::web::decode_bin_host_emit_js;

#[cfg(ui)]
pub use crate::key_stroke::PressedKey;

pub use crate::list_nav::{MenuDirection, choice_number_index, move_selection};
