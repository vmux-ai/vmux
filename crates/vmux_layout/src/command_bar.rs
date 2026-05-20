#[cfg(target_arch = "wasm32")]
pub mod page;

pub mod keyboard;
pub mod results;
pub mod style;

#[cfg(not(target_arch = "wasm32"))]
pub mod handler;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod shortcut;
