//! Systems registered by [`crate::VmuxPlugin`](super::VmuxPlugin) and sub-plugins.

use bevy::prelude::*;

use crate::core::{CAMERA_DISTANCE, VmuxWorldCamera};

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
