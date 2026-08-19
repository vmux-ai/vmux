//! File viewer and editor page: loading, editing, syntax highlighting, file watching,
//! image preview, and LSP integration as a Dioxus page.

pub mod page_model;

#[cfg(ui)]
pub mod explorer;
#[cfg(ui)]
pub mod lsp_page;
#[cfg(ui)]
pub mod page;
#[cfg(ui)]
pub mod page_key;

#[cfg(ui)]
mod note;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
