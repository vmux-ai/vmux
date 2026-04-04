//! Browser composition crate for vmux.
//!
//! `BrowserPlugin` is the top-level browser stack, while `WebviewPlugin`
//! remains focused on CEF/webview rendering logic.

use bevy::prelude::*;
pub use vmux_webview::*;

pub use vmux_history::HistoryPlugin;
pub use vmux_ui_native::hosted::history::{HistoryUiBaseUrl, HistoryUiPlugin};
pub use vmux_server::ServerPlugin;
pub use vmux_status_bar::{StatusBarServerPlugin as StatusBarPlugin, StatusUiBaseUrl};
pub use vmux_ui_native::UiLibraryPlugin;

/// Top-level browser stack plugin.
#[derive(Default)]
pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ServerPlugin,
            WebviewPlugin::default(),
            StatusBarPlugin::default(),
            UiLibraryPlugin::default(),
            HistoryPlugin,
        ));
    }
}
