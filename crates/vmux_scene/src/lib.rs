//! vmux scene bootstrap: primary window tuning, world camera, directional light, macOS liquid glass.

#[cfg(target_os = "macos")]
mod macos_liquid_glass;
mod system;

use bevy::prelude::*;

/// Startup systems for the 3D scene: window, clear color, camera, light; chains into
/// [`vmux_command::setup`]. On macOS, also applies liquid-glass when available.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins(ScenePlugin);
    }
}
