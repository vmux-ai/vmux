pub mod event;

#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::ServicesPlugin;
