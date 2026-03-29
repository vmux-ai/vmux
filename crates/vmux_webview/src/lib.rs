//! Default CEF webview spawn and window/camera layout.

mod system;

use bevy::prelude::*;
use bevy::render::camera::camera_system;

pub use system::{
    fit_webview_plane_to_window, go_back, go_forward, sync_webview_layout_size_to_window,
};

/// Marker for the primary vmux webview entity.
#[derive(Component)]
pub struct VmuxWebview;

/// URL for the default webview plane.
pub const WEBVIEW_URL: &str = "https://github.com/not-elm/bevy_cef";

/// CEF page zoom; `0.0` matches typical desktop browsers at 100%.
pub const CEF_PAGE_ZOOM_LEVEL: f64 = 0.0;

#[derive(Default)]
pub struct VmuxWebviewPlugin;

impl Plugin for VmuxWebviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, system::spawn_webview)
            .add_systems(Update, (system::go_back, system::go_forward))
            .add_systems(
                PostUpdate,
                (
                    sync_webview_layout_size_to_window,
                    fit_webview_plane_to_window,
                )
                    .chain()
                    .after(camera_system),
            );
    }
}
