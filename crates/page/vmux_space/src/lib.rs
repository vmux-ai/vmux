#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod model;
#[cfg(ui)]
pub mod native_page;
#[cfg(ui)]
pub mod ui;

pub use vmux_api::space as event;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
