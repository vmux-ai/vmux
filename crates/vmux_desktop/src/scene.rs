use crate::{
    command::{AppCommand, SceneCommand, ReadAppCommands},
    layout::fit_window_to_screen,
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    camera::PerspectiveProjection,
    camera::Projection,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin, FreeCameraState},
    post_process::bloom::Bloom,
    prelude::*,
    window::PrimaryWindow,
};




pub(crate) const FOV_Y: f32 = std::f32::consts::FRAC_PI_4;

fn camera_margin_px(_settings: &AppSettings) -> f32 {
    0.0
}

#[derive(Component)]
pub(crate) struct MainCamera;

#[derive(Component)]
struct SceneSunlight;

#[derive(Default)]
pub struct ScenePlugin;



impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FreeCameraPlugin)
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup.after(load_settings))
            .add_systems(
                Startup,
                fit_main_camera
                    .after(load_settings)
                    .after(fit_window_to_screen),
            )
            .add_systems(
                Update,
                ((on_reset_camera, on_toggle_free_camera).in_set(ReadAppCommands),),
            )
            .add_systems(
                PostUpdate,
                fit_main_camera.after(fit_window_to_screen),
            );

        #[cfg(target_os = "macos")]
        app.insert_resource(ClearColor(Color::NONE));
    }
}

pub(crate) fn setup(
    mut commands: Commands,
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
) {
    let mut state = FreeCameraState::default();
    state.enabled = false;

    commands.spawn((
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: FOV_Y,
            ..default()
        }),
        frame_main_camera_transform(&window, window.aspect(), camera_margin_px(&settings)),
        FreeCamera {
            sensitivity: 1.0,
            friction: 25.0,
            walk_speed: 0.5,
            run_speed: 5.0,
            ..default()
        },
        state,
    ));
}

fn fit_main_camera(
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut camera_q: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
    camera_state: Single<&FreeCameraState, With<MainCamera>>,
) {
    let Ok((mut tf, mut proj)) = camera_q.single_mut() else {
        return;
    };
    let aspect = window.aspect();

    if let Projection::Perspective(ref mut p) = *proj {
        if (p.aspect_ratio - aspect).abs() > f32::EPSILON {
            p.aspect_ratio = aspect;
        }
    }

    if !camera_state.enabled {
        *tf = frame_main_camera_transform(&window, aspect, camera_margin_px(&settings));
    }
}

fn on_reset_camera(
    mut reader: MessageReader<AppCommand>,
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut transform: Single<&mut Transform, With<MainCamera>>,
    projection: Single<&Projection, With<MainCamera>>,
    mut camera_state: Single<&mut FreeCameraState, With<MainCamera>>,
    camera: Single<Entity, With<MainCamera>>,
    sunlight_q: Query<Entity, With<SceneSunlight>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Scene(SceneCommand::Reset) = *cmd else {
            continue;
        };

        camera_state.enabled = false;
        commands.entity(*camera).remove::<Bloom>();

        for e in &sunlight_q {
            commands.entity(e).despawn();
        }

        let aspect = match &*projection {
            Projection::Perspective(p) => p.aspect_ratio,
            _ => window.aspect(),
        };

        **transform = frame_main_camera_transform(&window, aspect, camera_margin_px(&settings));
    }
}

fn on_toggle_free_camera(
    mut reader: MessageReader<AppCommand>,
    mut state: Single<&mut FreeCameraState, With<MainCamera>>,
    camera: Single<Entity, With<MainCamera>>,
    sunlight_q: Query<Entity, With<SceneSunlight>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Scene(SceneCommand::ToggleFreeCamera) = *cmd else {
            continue;
        };
        state.enabled = !state.enabled;
        if state.enabled {
            commands.entity(*camera).insert(Bloom::NATURAL);
            commands.spawn((
                SceneSunlight,
                DirectionalLight {
                    illuminance: 8000.0,
                    shadows_enabled: false,
                    color: Color::srgb(1.0, 0.98, 0.95),
                    ..default()
                },
                Transform::from_rotation(Quat::from_euler(
                    EulerRot::XYZ,
                    -0.6,
                    0.4,
                    0.0,
                )),
            ));
        } else {
            commands.entity(*camera).remove::<Bloom>();
            for e in &sunlight_q {
                commands.entity(e).despawn();
            }
        }
    }
}

pub(crate) fn frame_main_camera_transform(
    window: &Window,
    aspect: f32,
    margin_px: f32,
) -> Transform {
    let m = window.meters();
    let margin = margin_px / PIXELS_PER_METER;
    let center = Vec3::new(0.0, m.y * 0.5, 0.0);

    let half_fov_y = FOV_Y * 0.5;
    let tan_half_fov_y = half_fov_y.tan();

    let dist_to_fit_height = ((m.y * 0.5) + margin) / tan_half_fov_y;
    let tan_half_fov_x = tan_half_fov_y * aspect;
    let dist_to_fit_width = ((m.x * 0.5) + margin) / tan_half_fov_x;
    let dist = dist_to_fit_height.max(dist_to_fit_width);

    Transform::from_xyz(center.x, center.y, center.z + dist).looking_at(center, Vec3::Y)
}
