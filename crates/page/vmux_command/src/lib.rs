#![cfg_attr(ui, allow(non_snake_case))]

#[cfg(ui)]
pub mod page;
#[cfg(ui)]
pub mod panel;

pub mod event;
pub mod size;
pub use vmux_wire::open_target;
pub use vmux_wire::prompt_media;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
