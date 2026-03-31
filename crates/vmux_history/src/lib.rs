//! History pane: **web UI** (Dioxus WASM in `dist/` via `wasm-bindgen`; native builds run `build.rs`)
//! + native [`HistoryServerPlugin`] and [`HistoryUiPlugin`].

pub const DIST_DIR_NAME: &str = "dist";
pub const DIST_WEB_DIR_NAME: &str = "web_dist";

#[cfg(not(target_arch = "wasm32"))]
mod embedded_web_dist;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{
    apply_toggle_history_pane, HistoryPlugin, HistoryServerPlugin, HistoryUiBaseUrl,
    HistoryUiPlugin, HistoryUiUrlReceiver,
};

#[cfg(not(target_arch = "wasm32"))]
pub use vmux_layout::{VmuxHostedWebPlugin, VmuxWebviewSurface};
