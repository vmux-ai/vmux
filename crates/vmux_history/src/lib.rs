//! History pane: **web UI** (Dioxus WASM in `dist/` via `wasm-bindgen`; native builds run `build.rs`)
//! + native [`server::HistoryServerPlugin`] and [`vmux_ui::hosted::history::HistoryUiPlugin`].

pub const DIST_DIR_NAME: &str = "dist";
pub const DIST_WEB_DIR_NAME: &str = "web_dist";

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;

/// Embedded history server + tiled history pane UI (adds [`server::HistoryServerPlugin`] and
/// [`HistoryUiPlugin`](vmux_ui::hosted::history::HistoryUiPlugin)).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub struct HistoryPlugin;

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            server::HistoryServerPlugin,
            vmux_ui::hosted::history::HistoryUiPlugin,
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use server::{HistoryServerPlugin, history_bundle_root};

#[cfg(not(target_arch = "wasm32"))]
pub use vmux_ui::hosted::history::{
    HistoryUiBaseUrl, HistoryUiEmitState, HistoryUiPlugin, HistoryUiUrlReceiver, OpenHistoryMode,
    apply_open_history_pane,
};

#[cfg(not(target_arch = "wasm32"))]
pub use vmux_layout::{VmuxHostedWebPlugin, VmuxWebviewSurface};
