//! Default CEF webview spawn and window/camera layout.

mod layout;
mod system;
mod tmux;

use bevy::prelude::*;
use bevy::render::camera::camera_system;

pub use layout::rebuild_session_snapshot;
pub use system::{go_back, go_forward, reload};
pub use vmux_layout::LayoutPlugin;

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
        app.add_plugins(LayoutPlugin);
        app.add_systems(Startup, layout::setup_vmux_panes)
            .add_systems(
                Update,
                (
                    system::go_back,
                    system::go_forward,
                    system::reload,
                    tmux::tmux_prefix_commands,
                    layout::split_active_pane,
                    layout::cycle_pane_focus,
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    layout::apply_pane_layout.after(camera_system),
                    layout::sync_cef_sizes_after_pane_layout.after(layout::apply_pane_layout),
                ),
            );
    }
}
