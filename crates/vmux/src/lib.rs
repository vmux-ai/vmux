//! vmux — Bevy + embedded CEF webview library.

pub mod core;
mod session;
mod system;

pub use core::{CAMERA_DISTANCE, VmuxWorldCamera};
pub use session::{SessionPlugin, vmux_cache_dir};
pub use vmux_core::{LastVisitedUrl, SessionSavePath};
pub use vmux_input::{AppAction, AppInputRoot, VmuxInputPlugin};
pub use vmux_layout::{LayoutPlugin, SessionLayoutSnapshot};
pub use vmux_webview::VmuxWebviewPlugin;

use bevy::prelude::*;
use bevy_cef::prelude::*;

/// User-writable CEF disk cache root (profiles, etc.).
pub fn cef_root_cache_path() -> Option<String> {
    session::vmux_cache_dir()
        .map(|p| p.join("cef").to_string_lossy().into_owned())
        .or_else(|| {
            std::env::temp_dir()
                .to_str()
                .map(|p| format!("{p}/vmux_cef"))
        })
}

#[derive(Default)]
pub struct VmuxScenePlugin;

impl Plugin for VmuxScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (system::spawn_camera, system::spawn_directional_light),
        );
    }
}

/// Full vmux stack: Bevy defaults, CEF, input, scene, and webview.
#[derive(Default)]
pub struct VmuxPlugin;

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        let cef_plugin = CefPlugin {
            command_line_config: CommandLineConfig {
                switches: vec![],
                switch_values: vec![],
            },
            root_cache_path: cef_root_cache_path(),
            ..Default::default()
        };
        app.add_plugins((
            DefaultPlugins,
            cef_plugin,
            VmuxInputPlugin,
            VmuxScenePlugin,
            JsEmitEventPlugin::<vmux_core::WebviewDocumentUrlEmit>::default(),
            SessionPlugin,
            VmuxWebviewPlugin,
        ));
    }
}
