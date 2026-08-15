use crate::{
    LayoutStartupSet, fit_window_to_screen,
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    camera::{OrthographicProjection, Projection, ScalingMode},
    prelude::*,
    window::PrimaryWindow,
};

#[derive(Default)]
pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup.in_set(LayoutStartupSet::Window))
            .add_systems(PostUpdate, fit_main_camera.after(fit_window_to_screen))
            .add_systems(
                PostUpdate,
                sync_camera_render_target.before(bevy::ui::UiSystems::Prepare),
            );

        #[cfg(target_os = "macos")]
        app.insert_resource(ClearColor(Color::NONE));
    }
}

#[derive(Component)]
pub struct MainCamera;

pub fn setup(mut commands: Commands, window: Single<&Window, With<PrimaryWindow>>) {
    let mut projection = OrthographicProjection::default_2d();
    projection.scaling_mode = ScalingMode::FixedVertical {
        viewport_height: window.meters().y.max(1.0 / PIXELS_PER_METER),
    };
    commands.spawn((
        MainCamera,
        Camera2d,
        Projection::Orthographic(projection),
        frame_main_camera_transform(&window, window.aspect(), 0.0),
    ));
}

/// Tell the camera how big its render target is, which nothing else does any more.
///
/// `bevy_ui` sizes every percentage-based node from `Camera::physical_viewport_size`, and that
/// reads `computed.target_info` — filled in by `camera_system`, which lived in `bevy_render`.
/// With the render stack gone nothing writes it, so every root node resolves to zero, and
/// `sync_children_to_ui` divides by that to get a scale of zero, at which point
/// `sync_windowed_frames` classes every pane as hidden and never gives it a frame. The window is
/// the render target here, so mirroring its resolution is the whole of what was lost.
fn sync_camera_render_target(
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera_q: Query<&mut bevy::camera::Camera, With<MainCamera>>,
) {
    let Ok(mut camera) = camera_q.single_mut() else {
        return;
    };
    let physical_size = UVec2::new(
        window.resolution.physical_width(),
        window.resolution.physical_height(),
    );
    let scale_factor = window.resolution.scale_factor();
    let unchanged = camera.computed.target_info.as_ref().is_some_and(|info| {
        info.physical_size == physical_size && info.scale_factor == scale_factor
    });
    if unchanged {
        return;
    }
    camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
        physical_size,
        scale_factor,
    });
}

fn fit_main_camera(
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera_q: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Ok((mut transform, mut projection)) = camera_q.single_mut() else {
        return;
    };
    if let Projection::Orthographic(projection) = &mut *projection {
        projection.scaling_mode = ScalingMode::FixedVertical {
            viewport_height: window.meters().y.max(1.0 / PIXELS_PER_METER),
        };
    }
    *transform = frame_main_camera_transform(&window, window.aspect(), 0.0);
}

pub fn frame_main_camera_transform(window: &Window, _aspect: f32, _margin_px: f32) -> Transform {
    Transform::from_xyz(0.0, window.meters().y * 0.5, 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_camera_uses_fixed_vertical_projection() {
        let mut app = App::new();
        app.add_systems(Update, setup);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.update();

        let projection = app
            .world_mut()
            .query_filtered::<&Projection, With<MainCamera>>()
            .single(app.world())
            .expect("main camera projection");

        assert!(matches!(
            projection,
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical { .. },
                ..
            })
        ));
    }
}
