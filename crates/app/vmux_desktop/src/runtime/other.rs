use bevy::prelude::*;

pub(super) struct RuntimePlatformPlugin;

impl Plugin for RuntimePlatformPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, activate_app_during_boot)
            .add_systems(Update, grab_key_window_on_pane_hover)
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

pub(super) fn live_resize_active() -> bool {
    false
}

pub(super) fn native_pointer_inside() -> bool {
    false
}
