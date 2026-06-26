pub mod state;

#[cfg(target_arch = "wasm32")]
pub mod page;

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
