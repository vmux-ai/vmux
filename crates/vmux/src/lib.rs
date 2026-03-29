//! vmux — Bevy + embedded CEF webview library.

mod component;
pub mod event;
mod system;

pub use component::{AppAction, AppInputRoot, VmuxWebview, VmuxWorldCamera};

use bevy::prelude::*;
use bevy::render::camera::camera_system;
use bevy_cef::prelude::*;
use leafwing_input_manager::prelude::*;

/// Z distance of the world camera from the webview plane at z = 0 (used for frustum sizing).
pub const CAMERA_DISTANCE: f32 = 3.0;

/// URL for the default webview plane.
pub const WEBVIEW_URL: &str = "https://github.com/not-elm/bevy_cef";

/// CEF page zoom; `0.0` matches typical desktop browsers at 100%.
pub const CEF_PAGE_ZOOM_LEVEL: f64 = 0.0;

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
pub struct VmuxInputPlugin;

impl Plugin for VmuxInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<AppAction>::default())
            .add_systems(Startup, system::spawn_app_input)
            .add_systems(Update, system::exit_on_quit_action);
    }
}

#[derive(Default)]
pub struct VmuxScenePlugin;

impl Plugin for VmuxScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                system::spawn_camera,
                system::spawn_directional_light,
                system::spawn_webview,
            ),
        );
    }
}

#[derive(Default)]
pub struct VmuxWebviewLayoutPlugin;

impl Plugin for VmuxWebviewLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (
                system::sync_webview_layout_size_to_window,
                system::fit_webview_plane_to_window,
            )
                .chain()
                .after(camera_system),
        );
    }
}

/// Full default vmux stack (input, scene, webview layout).
///
/// Add together with [`CefPlugin`] and Bevy [`DefaultPlugins`].
#[derive(Default)]
pub struct VmuxPlugin;

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VmuxInputPlugin::default(),
            VmuxScenePlugin::default(),
            VmuxWebviewLayoutPlugin::default(),
        ));
    }
}
