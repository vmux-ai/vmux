//! vmux — Bevy + embedded CEF webview library.

pub use vmux_core::{CAMERA_DISTANCE, VmuxWorldCamera};
pub use vmux_command::CommandPlugin;
pub use vmux_input::{AppCommand, AppInputRoot, InputPlugin, KeyAction};
pub use vmux_layout::LastVisitedUrl;
pub use vmux_layout::{LayoutPlugin, SessionLayoutSnapshot};
pub use vmux_scene::ScenePlugin;
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
