#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
pub mod schema;
pub mod themes;
pub mod ui_state;

#[cfg(ui)]
pub mod page;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
