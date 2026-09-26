#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
pub mod protocol;
pub mod reconcile;
pub mod state;

#[cfg(ui)]
pub mod ui;

#[cfg(ui)]
mod extension;

#[cfg(ui)]
mod remote;

#[cfg(ui)]
pub mod error_page;
#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
