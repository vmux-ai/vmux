//! Default CEF webview spawn and window/camera layout.

mod system;

use bevy::prelude::*;
use bevy_cef::prelude::{CefExtensions, CefPlugin, CommandLineConfig, JsEmitEventPlugin};
pub use vmux_layout::{VmuxHostedWebPlugin, VmuxWebviewSurface};
pub use vmux_server::{
    EmbeddedServeDirRequest, EmbeddedServeDirStartup, PendingEmbeddedServeDir, VmuxServerPlugin,
    VmuxServerShutdownRegistry, register_shutdown_flag, spawn_embedded_serve_dir_system,
};
pub use vmux_status_bar::{StatusBarHostedPlugin, StatusUiBaseUrl};
pub use system::{go_back, go_forward, reload};
pub use vmux_layout::{CEF_PAGE_ZOOM_LEVEL, LayoutPlugin, VmuxWebview, rebuild_session_snapshot};
pub use vmux_settings::{VmuxAppSettings, cef_root_cache_path, default_webview_url};

/// Webview stack plus [`CefPlugin`] configuration (command line, extensions, CEF cache root).
#[derive(Clone, Debug)]
pub struct VmuxWebviewPlugin {
    pub command_line_config: CommandLineConfig,
    pub extensions: CefExtensions,
    pub root_cache_path: Option<String>,
}

impl Default for VmuxWebviewPlugin {
    fn default() -> Self {
        Self {
            command_line_config: CommandLineConfig::default(),
            extensions: CefExtensions::default(),
            root_cache_path: vmux_settings::cef_root_cache_path(),
        }
    }
}

impl Plugin for VmuxWebviewPlugin {
    fn build(&self, app: &mut App) {
        let cef_plugin = CefPlugin {
            command_line_config: self.command_line_config.clone(),
            extensions: self.extensions.clone(),
            root_cache_path: self.root_cache_path.clone(),
        };
        app.add_plugins((
            VmuxServerPlugin,
            cef_plugin,
            LayoutPlugin,
            JsEmitEventPlugin::<vmux_core::WebviewDocumentUrlEmit>::default(),
            StatusBarHostedPlugin,
        ));
        app.add_systems(
            Update,
            (system::go_back, system::go_forward, system::reload),
        );
    }
}
