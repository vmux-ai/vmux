//! Systems registered by [`crate::VmuxPlugin`](super::VmuxPlugin) and sub-plugins.

use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};

use crate::core::{CAMERA_DISTANCE, VmuxWorldCamera};

/// Prefer high-refresh / low-latency swap chains (Immediate → Mailbox → Fifo) when available.
pub(crate) fn configure_primary_window_present_mode(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    for mut window in &mut windows {
        window.present_mode = PresentMode::AutoNoVsync;
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
