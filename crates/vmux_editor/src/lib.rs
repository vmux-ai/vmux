pub mod viewport;

#[cfg(not(target_arch = "wasm32"))]
pub mod highlight;

#[cfg(not(target_arch = "wasm32"))]
mod dir;
#[cfg(not(target_arch = "wasm32"))]
mod preview;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{EditorPlugin, FileView, restore_file_view_bundle};

#[cfg(any(target_arch = "wasm32", test))]
pub mod page_model;

#[cfg(target_arch = "wasm32")]
mod lang_icon;
#[cfg(target_arch = "wasm32")]
pub mod page;
