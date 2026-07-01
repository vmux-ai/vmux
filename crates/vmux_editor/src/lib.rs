//! File viewer and editor page: loading, editing, syntax highlighting, file watching,
//! image preview, and LSP integration in a CEF + Dioxus webview.

pub mod viewport;

#[cfg(not(target_arch = "wasm32"))]
pub mod highlight;

#[cfg(not(target_arch = "wasm32"))]
pub mod edit;
#[cfg(not(target_arch = "wasm32"))]
pub mod fold;
#[cfg(not(target_arch = "wasm32"))]
pub mod fold_store;
#[cfg(not(target_arch = "wasm32"))]
pub mod keymap;

#[cfg(not(target_arch = "wasm32"))]
mod dir;
#[cfg(not(target_arch = "wasm32"))]
mod preview;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{EditorPlugin, FileView, restore_file_view_bundle};

#[cfg(not(target_arch = "wasm32"))]
pub mod lsp;
#[cfg(not(target_arch = "wasm32"))]
pub use lsp::LspPlugin;

#[cfg(any(target_arch = "wasm32", test))]
pub mod page_model;

#[cfg(target_arch = "wasm32")]
pub mod lsp_page;
#[cfg(target_arch = "wasm32")]
pub mod page;
