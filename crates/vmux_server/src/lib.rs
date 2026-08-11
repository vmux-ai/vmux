//! Serves embedded webview page bundles over `vmux://` URLs on the host, and on wasm
//! dispatches the web build to the correct per-host Dioxus page.

pub mod page_host;

#[cfg(feature = "build")]
pub mod build;

#[cfg(all(web, feature = "web"))]
mod web;

#[cfg(all(web, feature = "web"))]
pub use web::App;

#[cfg(not(web))]
pub use vmux_core::page::{
    PAGE_READY_BIN_EVENT_ID, PageManifest, PageReady, ServerEmbedSet, ServerPlugin,
    mark_webview_page_ready,
};
