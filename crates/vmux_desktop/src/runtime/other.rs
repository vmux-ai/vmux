//! Non-macOS runtime. There is no AppKit event monitor to install and nothing owns the
//! pointer behind winit's back, so activation and wake systems are no-ops and every frame
//! is rendered.

use bevy::prelude::*;

use super::RenderFrameDemand;

/// The non-macOS half of [`super::RuntimePlugin`]: every system it registers is a no-op except render demand, which is always on.
pub(super) struct RuntimePlatformPlugin;

impl Plugin for RuntimePlatformPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, activate_app_during_boot)
            .add_systems(Update, grab_key_window_on_pane_hover)
            .add_systems(Last, sync_render_frame_demand)
            .add_systems(
                Startup,
                (
                    install_native_mouse_wake_monitor,
                    install_live_resize_monitor,
                    activate_primary_window_on_startup,
                ),
            );
    }
}

fn activate_primary_window_on_startup() {}

fn grab_key_window_on_pane_hover() {}

fn activate_app_during_boot() {}

fn install_native_mouse_wake_monitor() {}

fn install_live_resize_monitor() {}

fn sync_render_frame_demand(mut demand: ResMut<RenderFrameDemand>) {
    demand.0 = true;
}

pub(super) fn live_resize_active() -> bool {
    false
}

pub(super) fn native_pointer_inside() -> bool {
    false
}
