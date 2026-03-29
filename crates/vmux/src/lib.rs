//! vmux — Bevy + embedded CEF webview library.

pub mod core;
mod system;

pub use core::{CAMERA_DISTANCE, VmuxWorldCamera};
pub use vmux_input::{AppAction, AppInputRoot, VmuxInputPlugin};
pub use vmux_webview::VmuxWebviewPlugin;

use bevy::prelude::*;
use bevy_cef::prelude::*;

/// User-writable CEF disk cache root (profiles, etc.).
pub fn cef_root_cache_path() -> Option<String> {
    if let Ok(home) = std::env::var("HOME") {
        let subdir = if cfg!(target_os = "macos") {
            "Library/Caches/vmux/cef"
        } else {
            ".cache/vmux/cef"
        };
        return Some(format!("{home}/{subdir}"));
    }
    std::env::temp_dir()
        .to_str()
        .map(|p| format!("{p}/vmux_cef"))
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
            VmuxInputPlugin::default(),
            VmuxScenePlugin::default(),
            VmuxWebviewPlugin::default(),
        ));
    }
}
