//! Status bar for vmux: **WASM UI** (`src/main.rs`, built with `dx`) + **native** [`StatusBarHostedPlugin`].
//!
//! ## Exports
//! - **UI:** build with `dx build --platform web` (output in [`DIST_DIR_NAME`]). Served by [`StatusBarHostedPlugin`].
//! - **Server plugin:** [`StatusBarHostedPlugin`] — embedded HTTP + chrome IPC.

/// Relative directory name for the Dioxus web bundle (`dx` writes here).
pub const DIST_DIR_NAME: &str = "dist";

#[cfg(not(target_arch = "wasm32"))]
mod plugin;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{StatusBarHostedPlugin, StatusUiBaseUrl, StatusUiUrlReceiver};

#[cfg(not(target_arch = "wasm32"))]
pub use vmux_layout::{VmuxHostedWebPlugin, VmuxWebviewSurface};
