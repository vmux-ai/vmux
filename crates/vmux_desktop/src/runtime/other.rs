//! Non-macOS runtime. There is no AppKit event monitor to install and nothing owns the
//! pointer behind winit's back, so activation and wake systems are no-ops and every frame
//! is rendered.

use bevy::prelude::*;

use super::RenderFrameDemand;

pub(super) fn activate_primary_window_on_startup() {}

pub(super) fn grab_key_window_on_pane_hover() {}

pub(super) fn activate_app_during_boot() {}

pub(super) fn install_native_mouse_wake_monitor() {}

pub(super) fn install_live_resize_monitor() {}

pub(super) fn sync_render_frame_demand(mut demand: ResMut<RenderFrameDemand>) {
    demand.0 = true;
}

pub(super) fn live_resize_active() -> bool {
    false
}

pub(super) fn native_pointer_inside() -> bool {
    false
}
