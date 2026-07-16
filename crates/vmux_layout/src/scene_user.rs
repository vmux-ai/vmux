use crate::{
    LayoutStartupSet, fit_window_to_screen,
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    camera::{OrthographicProjection, Projection, ScalingMode},
    prelude::*,
    window::PrimaryWindow,
};

const TRANSITION_DURATION: f32 = 0.3;

#[derive(Component)]
pub struct MainCamera;

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub enum InteractionMode {
    #[default]
    User,
    Player,
}

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum SceneSystems {
    CompleteModeTransition,
}

#[derive(Resource)]
pub struct CameraHome(pub Transform);

#[derive(Resource)]
pub struct ModeTransition {
    pub direction: TransitionDirection,
    pub timer: Timer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TransitionDirection {
    EnterPlayer,
    ExitPlayer,
}

impl ModeTransition {
    pub fn new(direction: TransitionDirection) -> Self {
        Self {
            direction,
            timer: Timer::from_seconds(TRANSITION_DURATION, TimerMode::Once),
        }
    }

    pub fn progress(&self) -> f32 {
        self.timer.fraction()
    }
}

#[derive(Default)]
pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InteractionMode>()
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup.in_set(LayoutStartupSet::Window))
            .add_systems(PostUpdate, fit_main_camera.after(fit_window_to_screen));

        #[cfg(target_os = "macos")]
        app.insert_resource(ClearColor(Color::NONE));
    }
}

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
