#![cfg_attr(ui, allow(non_snake_case))]

extern crate self as vmux_command;

#[cfg(ui)]
pub mod page;
#[cfg(ui)]
pub mod panel;

pub mod event;
pub mod size;
pub use vmux_api::open_target;
pub use vmux_api::prompt_media;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
