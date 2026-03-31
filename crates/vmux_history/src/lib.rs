//! History overlay: **web UI** (Dioxus WASM in `web_dist/` via `wasm-bindgen`; native builds run `build.rs`)
//! + native [`VmuxHistoryServerPlugin`] and [`VmuxHistoryUiPlugin`].

pub const DIST_DIR_NAME: &str = "dist";
pub const DIST_WEB_DIR_NAME: &str = "web_dist";

#[cfg(not(target_arch = "wasm32"))]
mod embedded_web_dist;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{
    apply_toggle_history_pane, HistoryUiBaseUrl, HistoryUiUrlReceiver, VmuxHistoryServerPlugin,
    VmuxHistoryUiPlugin,
};

#[cfg(not(target_arch = "wasm32"))]
pub use vmux_layout::{VmuxHostedWebPlugin, VmuxWebviewSurface};
