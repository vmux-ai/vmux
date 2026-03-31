//! Systems registered by [`crate::ScenePlugin`](super::ScenePlugin) and sub-plugins.

#[cfg(target_os = "macos")]
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, PresentMode, PrimaryWindow};
use vmux_layout::{CAMERA_DISTANCE, VmuxWorldCamera};
use vmux_settings::VmuxAppSettings;

/// Older moonshine saves may omit `window_padding_top_px` (loads as 0). Mirror `window_padding_px`.
pub(crate) fn normalize_window_padding_from_legacy_save(mut settings: ResMut<VmuxAppSettings>) {
    let layout = &mut settings.layout;
    if layout.window_padding_top_px <= 0.0 && layout.window_padding_px > 0.0 {
        layout.window_padding_top_px = layout.window_padding_px;
    }
}

/// Present mode, and on macOS hide the title bar while allowing drag from the window background.
pub(crate) fn configure_primary_window(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    for mut window in &mut windows {
        window.present_mode = PresentMode::AutoNoVsync;
        #[cfg(target_os = "macos")]
        {
            window.titlebar_shown = false;
            window.movable_by_window_background = true;
            // Re-sync in case anything reset defaults; transparency + PostMultiplied is required
            // for wgpu to configure a non-opaque CAMetalLayer (see `wgpu-hal` patch).
            window.transparent = true;
            window.composite_alpha_mode = CompositeAlphaMode::PostMultiplied;
            window.titlebar_transparent = true;
            window.fullsize_content_view = true;
        }
    }
}

/// Sets [`ClearColor`] from the primary [`Window`]: transparent windows use `Color::NONE` so the
/// compositor can show content behind; otherwise Bevy’s default clear color.
pub(crate) fn sync_clear_color_from_primary_window(
    mut clear: ResMut<ClearColor>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    clear.0 = if window.transparent {
        Color::NONE
    } else {
        ClearColor::default().0
    };
}

pub(crate) fn spawn_camera(mut commands: Commands) {
    #[cfg(target_os = "macos")]
    {
        // TonyMcMapface + transparent clears tends to read flat/bright; None keeps linear color
        // for the compositor so window blur + light clear tint look like frosted glass.
        commands.spawn((
            VmuxWorldCamera,
            Camera3d::default(),
            Tonemapping::None,
            DebandDither::Disabled,
            Transform::from_translation(Vec3::new(0., 0., CAMERA_DISTANCE))
                .looking_at(Vec3::ZERO, Vec3::Y),
        ));
    }
    #[cfg(not(target_os = "macos"))]
    {
        commands.spawn((
            VmuxWorldCamera,
            Camera3d::default(),
            Transform::from_translation(Vec3::new(0., 0., CAMERA_DISTANCE))
                .looking_at(Vec3::ZERO, Vec3::Y),
        ));
    }
}

pub(crate) fn spawn_directional_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_translation(Vec3::new(1., 1., 1.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
