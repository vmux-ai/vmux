pub mod state;

#[cfg(not(target_arch = "wasm32"))]
pub mod event;
#[cfg(not(target_arch = "wasm32"))]
pub mod handler;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;

#[cfg(not(target_arch = "wasm32"))]
pub use event::Island;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::IslandPlugin;
