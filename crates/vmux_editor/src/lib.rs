//! File viewer and editor page: loading, editing, syntax highlighting, file watching,
//! image preview, and LSP integration in a CEF + Dioxus webview.

pub mod page_model;

#[cfg(ui)]
pub mod lsp_page;

// `web`, not `ui`: these reach the DOM directly and are only served into the CEF webview. They
// were written when `ui` and `web` were the same thing, which stopped being true when iOS arrived.
#[cfg(web)]
pub mod explorer;
#[cfg(web)]
pub mod page;
#[cfg(web)]
pub mod page_key;

#[cfg(web)]
mod note;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
