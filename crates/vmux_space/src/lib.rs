pub mod migration;
pub mod model;

pub use vmux_core::event::space as event;

#[cfg(not(target_arch = "wasm32"))]
pub mod cwd;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod spaces;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::SpacePlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use spaces::{ActiveSpace, SpacesView, active_space_rows, read_space_registry_from};
