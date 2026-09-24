#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
pub mod render_model;

#[cfg(ui)]
pub mod page;
#[cfg(ui)]
mod page_state;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
