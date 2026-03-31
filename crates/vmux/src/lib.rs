//! vmux — Bevy + embedded CEF webview library.

pub mod core;
#[cfg(target_os = "macos")]
mod macos_liquid_glass;
mod system;

pub use core::{CAMERA_DISTANCE, VmuxWorldCamera};
pub use vmux_command::CommandPlugin;
pub use vmux_input::{AppCommand, AppInputRoot, InputPlugin};
pub use vmux_layout::LastVisitedUrl;
pub use vmux_layout::{LayoutPlugin, SessionLayoutSnapshot};
pub use vmux_session::SessionPlugin;
pub use vmux_session::{SessionSavePath, SessionSaveQueue};
pub use vmux_settings::cef_root_cache_path;
pub use vmux_settings::{SettingsPlugin, VmuxAppSettings};
pub use vmux_browser::{BrowserPlugin, WebviewPlugin};

use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, Window, WindowPlugin};

/// Primary window: on macOS, transparent surface + post-multiplied alpha for system compositor
/// (see [Bevy window docs](https://docs.rs/bevy/latest/bevy/window/struct.Window.html#structfield.transparent)).
#[cfg(target_os = "macos")]
fn vmux_primary_window() -> Window {
    Window {
        transparent: true,
        composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
        // Match liquid-glass / NSGlassEffectView expectations (winit macOS extensions).
        titlebar_transparent: true,
        fullsize_content_view: true,
        ..default()
    }
}

#[cfg(not(target_os = "macos"))]
fn vmux_primary_window() -> Window {
    Window::default()
}

#[derive(Default)]
pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                system::normalize_window_padding_from_legacy_save,
                system::configure_primary_window,
                system::sync_clear_color_from_primary_window,
                system::spawn_camera,
                vmux_command::setup.after(system::spawn_camera),
                system::spawn_directional_light,
            )
                .chain(),
        );
        #[cfg(target_os = "macos")]
        app.add_systems(Update, macos_liquid_glass::apply_macos_liquid_glass);
    }
}

/// Full vmux stack: Bevy defaults, CEF, input, scene, and webview.
#[derive(Default)]
pub struct VmuxPlugin;

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            DefaultPlugins
                .set(WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(WindowPlugin {
                    primary_window: Some(vmux_primary_window()),
                    ..default()
                }),
            SettingsPlugin,
            InputPlugin,
            CommandPlugin,
            LayoutPlugin,
            ScenePlugin,
            SessionPlugin,
            BrowserPlugin::default(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins(ScenePlugin);
    }
}
