//! Terminal page: spawns and drives shell processes through the background service and
//! renders them in a CEF + Dioxus terminal webview.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
pub mod render_model;

#[cfg(ui)]
pub mod matrix_rain;
#[cfg(ui)]
pub mod page;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
