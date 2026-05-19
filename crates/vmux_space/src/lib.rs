#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod model;

pub use vmux_core::event::space as event;

#[cfg(not(target_arch = "wasm32"))]
pub mod cwd;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod spaces;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{SaveSpaceRequest, SpacePlugin};
#[cfg(not(target_arch = "wasm32"))]
pub use spaces::{ActiveSpace, Spaces, active_space_rows, read_space_registry_from};
