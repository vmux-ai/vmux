#[cfg(target_arch = "wasm32")]
pub mod page;
#[cfg(target_arch = "wasm32")]
pub mod palette;

pub use vmux_start::{keyboard, results, style};

pub mod size;

#[cfg(not(target_arch = "wasm32"))]
pub mod handler;
#[cfg(not(target_arch = "wasm32"))]
pub mod panel;
#[cfg(not(target_arch = "wasm32"))]
pub mod shortcut;
#[cfg(not(target_arch = "wasm32"))]
pub mod state;
#[cfg(not(target_arch = "wasm32"))]
pub mod work_snapshot;
