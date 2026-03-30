//! Systems registered by [`crate::VmuxPlugin`](super::VmuxPlugin) and sub-plugins.

use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};

#[cfg(target_os = "macos")]
use bevy_winit::WINIT_WINDOWS;

use crate::core::{CAMERA_DISTANCE, VmuxWorldCamera};

/// Present mode, and on macOS hide the title bar while allowing drag from the window background.
pub(crate) fn configure_primary_window(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    for mut window in &mut windows {
        window.present_mode = PresentMode::AutoNoVsync;
        #[cfg(target_os = "macos")]
        {
            window.titlebar_shown = false;
            window.movable_by_window_background = true;
        }
    }
}

pub(crate) fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        VmuxWorldCamera,
        Camera3d::default(),
        Transform::from_translation(Vec3::new(0., 0., CAMERA_DISTANCE))
            .looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

pub(crate) fn spawn_directional_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_translation(Vec3::new(1., 1., 1.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Enables winit’s macOS window blur (`CGSSetWindowBackgroundBlurRadius`) once the native window exists.
/// Uses Bevy’s thread-local [`WINIT_WINDOWS`] (same as `bevy_winit`).
#[cfg(target_os = "macos")]
pub(crate) fn apply_macos_window_blur(
    primary: Query<Entity, With<PrimaryWindow>>,
    mut attempts: Local<u32>,
) {
    const MAX_ATTEMPTS: u32 = 180;
    if *attempts >= MAX_ATTEMPTS {
        return;
    }
    let Ok(entity) = primary.single() else {
        return;
    };
    WINIT_WINDOWS.with(|cell| {
        let windows = cell.borrow();
        if let Some(winit) = windows.get_window(entity) {
            winit.set_blur(true);
            *attempts = MAX_ATTEMPTS;
        } else {
            *attempts += 1;
        }
    });
}
