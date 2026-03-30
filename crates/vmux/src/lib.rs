//! vmux — Bevy + embedded CEF webview library.

pub mod core;
mod system;

pub use core::{CAMERA_DISTANCE, VmuxWorldCamera};
pub use vmux_session::{SessionSavePath, SessionSaveQueue};
pub use vmux_input::{AppAction, AppInputRoot, VmuxInputPlugin};
pub use vmux_layout::LastVisitedUrl;
pub use vmux_layout::{LayoutPlugin, SessionLayoutSnapshot};
pub use vmux_session::SessionPlugin;
pub use vmux_settings::cef_root_cache_path;
pub use vmux_settings::{SettingsPlugin, VmuxAppSettings};
pub use vmux_webview::VmuxWebviewPlugin;

use bevy::prelude::*;

#[derive(Default)]
pub struct VmuxScenePlugin;

impl Plugin for VmuxScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                system::configure_primary_window_present_mode,
                system::spawn_camera,
                system::spawn_directional_light,
            ),
        );
    }
}

/// Full vmux stack: Bevy defaults, CEF, input, scene, and webview.
#[derive(Default)]
pub struct VmuxPlugin;

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            DefaultPlugins,
            SettingsPlugin,
            VmuxInputPlugin,
            VmuxScenePlugin,
            SessionPlugin,
            VmuxWebviewPlugin::default(),
        ));
    }
}
