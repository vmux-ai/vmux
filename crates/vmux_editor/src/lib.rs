pub mod viewport;

#[cfg(not(target_arch = "wasm32"))]
pub mod highlight;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{EditorPlugin, FileView};

#[cfg(any(target_arch = "wasm32", test))]
pub mod page_model;

#[cfg(target_arch = "wasm32")]
pub mod page;
