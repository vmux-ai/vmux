#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;

mod projection;
pub mod roster;
#[cfg(host)]
mod tool;
#[cfg(host)]
pub use tool::TeamToolPlugin;
