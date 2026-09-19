#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod vibe;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
