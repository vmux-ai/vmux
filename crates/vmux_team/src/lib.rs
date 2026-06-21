#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{ActiveProfile, TeamPlugin};
