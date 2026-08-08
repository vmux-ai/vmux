//! File viewer and editor page: loading, editing, syntax highlighting, file watching,
//! image preview, and LSP integration in a CEF + Dioxus webview.

pub mod viewport;

#[cfg(not(web))]
pub mod highlight;

#[cfg(not(web))]
pub mod edit;
#[cfg(not(web))]
pub mod fold;
#[cfg(not(web))]
pub mod fold_store;
#[cfg(not(web))]
pub mod keymap;
#[cfg(not(web))]
pub mod markdown;
#[cfg(not(web))]
mod wrap;

#[cfg(not(web))]
mod dir;
#[cfg(not(web))]
mod explorer_fs;
#[cfg(not(web))]
pub mod explorer_model;
#[cfg(not(web))]
mod preview;

#[cfg(not(web))]
mod plugin;
#[cfg(not(web))]
pub use plugin::{
    EditorPlugin, FileView, FileViewModeRequest, GlobalSearchRequest, StackExplorerVisibility,
    restore_file_view_bundle,
};

#[cfg(not(web))]
pub mod lsp;
#[cfg(not(web))]
pub use lsp::LspPlugin;

#[cfg(any(web, test))]
pub mod page_model;

#[cfg(web)]
pub mod explorer;
#[cfg(web)]
pub mod lsp_page;
#[cfg(web)]
mod note;
#[cfg(web)]
pub mod page;
