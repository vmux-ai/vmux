//! Spaces (workspace groupings): CRUD over named spaces, per-space startup URL and
//! directory, active-space tracking, and the spaces list webview.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod model;
#[cfg(frontend)]
pub mod page;

pub use vmux_wire::space as event;

#[cfg(native)]
mod native;
#[cfg(native)]
pub use native::*;
