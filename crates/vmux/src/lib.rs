//! vmux — Bevy + embedded CEF webview library.

pub mod core;
mod system;

pub use core::{CAMERA_DISTANCE, VmuxWorldCamera};
pub use vmux_input::{AppAction, AppInputRoot, VmuxInputPlugin};
pub use vmux_layout::LastVisitedUrl;
pub use vmux_layout::{LayoutPlugin, SessionLayoutSnapshot};
pub use vmux_session::SessionPlugin;
pub use vmux_session::{SessionSavePath, SessionSaveQueue};
pub use vmux_settings::cef_root_cache_path;
pub use vmux_settings::{SettingsPlugin, VmuxAppSettings};
pub use vmux_webview::VmuxWebviewPlugin;

use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, Window, WindowPlugin};

/// Primary window: on macOS, transparent surface + post-multiplied alpha for system compositor
/// (see [Bevy window docs](https://docs.rs/bevy/latest/bevy/window/struct.Window.html#structfield.transparent)).
#[cfg(target_os = "macos")]
fn vmux_primary_window() -> Window {
    Window {
        transparent: true,
        composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
        ..default()
    }
}

#[cfg(not(target_os = "macos"))]
fn vmux_primary_window() -> Window {
    Window::default()
}

#[derive(Default)]
pub struct VmuxScenePlugin;

impl Plugin for VmuxScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                system::configure_primary_window,
                system::spawn_camera,
                system::spawn_directional_light,
            ),
        );
        #[cfg(target_os = "macos")]
        app.add_systems(Update, system::apply_macos_window_blur);
    }
}

/// Full vmux stack: Bevy defaults, CEF, input, scene, and webview.
#[derive(Default)]
pub struct VmuxPlugin;

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(vmux_primary_window()),
                ..default()
            }),
            SettingsPlugin,
            VmuxInputPlugin,
            VmuxScenePlugin,
            SessionPlugin,
            VmuxWebviewPlugin::default(),
        ));
        #[cfg(target_os = "macos")]
        {
            app.insert_resource(ClearColor(Color::NONE));
        }
    }
}
