#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(ui)]
pub mod page;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;

pub mod roster;
