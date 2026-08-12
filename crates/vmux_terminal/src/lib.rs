//! Terminal page: spawns and drives shell processes through the background service and
//! renders them in a CEF + Dioxus terminal webview.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
pub mod render_model;

// `web`, not `ui`: both reach the DOM directly and are only served into the CEF webview. They were
// written when `ui` and `web` were the same thing, which stopped being true when iOS arrived.
#[cfg(web)]
pub mod matrix_rain;
#[cfg(web)]
pub mod page;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
