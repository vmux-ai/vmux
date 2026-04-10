use crate::{
    command::{AppCommand, CameraCommand, ReadAppCommands},
    layout3::fit_display_glass_to_window,
    settings::load_settings,
    unit::WindowExt,
};
use bevy::{
    camera::PerspectiveProjection,
    camera::Projection,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin, FreeCameraState},
    post_process::bloom::Bloom,
    prelude::*,
    window::PrimaryWindow,
};
use bevy_infinite_grid::{InfiniteGridBundle, InfiniteGridPlugin};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

#[cfg(target_os = "macos")]
use bevy::window::RawHandleWrapper;
#[cfg(target_os = "macos")]
use liquid_glass_rs::{GlassOptions, GlassViewManager};
#[cfg(target_os = "macos")]
use raw_window_handle::RawWindowHandle;
#[cfg(target_os = "macos")]
use std::marker::PhantomData;
#[cfg(target_os = "macos")]
use std::rc::Rc;

pub(crate) const FOV_Y: f32 = std::f32::consts::FRAC_PI_4;
const BOUNCE_DISPLAY_CLEARANCE: f32 = 0.9;

#[derive(Component)]
pub(crate) struct MainCamera;

#[derive(Component)]
struct Bouncing;

#[derive(Default)]
pub struct ScenePlugin;

#[cfg(target_os = "macos")]
struct LiquidGlassMainThread(PhantomData<Rc<()>>);

#[cfg(target_os = "macos")]
impl Default for LiquidGlassMainThread {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FreeCameraPlugin, InfiniteGridPlugin))
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup.after(load_settings))
            .add_systems(
                Startup,
                (fit_main_camera, spawn_bloom)
                    .chain()
                    .after(load_settings)
                    .after(fit_display_glass_to_window),
            )
            .add_systems(
                Update,
                ((on_reset_camera, on_toggle_free_camera).in_set(ReadAppCommands),),
            )
            .add_systems(
                PostUpdate,
                fit_main_camera.after(fit_display_glass_to_window),
            );

        #[cfg(target_os = "macos")]
        app.insert_resource(ClearColor(Color::NONE))
            .insert_non_send_resource(LiquidGlassMainThread::default())
            .add_systems(Update, apply_liquid_glass);
    }
}

pub(crate) fn setup(mut commands: Commands, window: Single<&Window, With<PrimaryWindow>>) {
    commands.spawn(InfiniteGridBundle::default());

    let mut state = FreeCameraState::default();
    state.enabled = false;

    commands.spawn((
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: FOV_Y,
            ..default()
        }),
        frame_main_camera_transform(&window, window.aspect()),
        FreeCamera {
            sensitivity: 0.2,
            friction: 25.0,
            walk_speed: 0.5,
            run_speed: 2.5,
            ..default()
        },
        state,
        Bloom::NATURAL,
    ));
}

fn spawn_bloom(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let m = window.meters();
    let aspect = window.aspect();

    let tan_half_fov = (FOV_Y * 0.5).tan();

    let d_v = (m.y * 0.5) / tan_half_fov;
    let d_h = (m.x * 0.5) / (tan_half_fov * aspect);

    let dist = d_v.max(d_h);
    let center = Vec3::new(0.0, m.y * 0.5, 0.0);
    let camera_pos = Vec3::new(center.x, center.y, center.z + dist);

    let mats = [
        materials.add(StandardMaterial {
            emissive: LinearRgba::rgb(0.0, 0.0, 150.0),
            ..default()
        }),
        materials.add(StandardMaterial {
            emissive: LinearRgba::rgb(1000.0, 1000.0, 1000.0),
            ..default()
        }),
        materials.add(StandardMaterial {
            emissive: LinearRgba::rgb(50.0, 0.0, 0.0),
            ..default()
        }),
        materials.add(StandardMaterial {
            base_color: Color::BLACK,
            ..default()
        }),
    ];

    let bounce_mesh = meshes.add(Sphere::new(0.35));
    let clear_radius_sq = camera_pos.distance_squared(center);

    for x in -5..5 {
        for z in -5..5 {
            let pos = Vec3::new(x as f32 * 2.0, 0.0, z as f32 * 2.0);
            if (pos - camera_pos).length_squared() < clear_radius_sq {
                continue;
            }

            let half_m = m * 0.5;
            let delta = pos.xz().abs() - half_m;
            let outside = delta.max(Vec2::ZERO);

            if outside.length() < BOUNCE_DISPLAY_CLEARANCE {
                continue;
            }

            let mut hasher = DefaultHasher::new();
            (x, z).hash(&mut hasher);
            let hash = hasher.finish();

            let (mat_idx, scale) = match hash % 6 {
                0 => (0, 0.5),
                1 => (1, 0.1),
                2 => (2, 1.0),
                _ => (3, 1.5),
            };

            commands.spawn((
                Mesh3d(bounce_mesh.clone()),
                MeshMaterial3d(mats[mat_idx].clone()),
                Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
                Bouncing,
            ));
        }
    }
}

fn fit_main_camera(
    window: Single<&Window, With<PrimaryWindow>>,
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
        *tf = frame_main_camera_transform(&window, aspect);
    }
}

fn on_reset_camera(
    mut reader: MessageReader<AppCommand>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut transform: Single<&mut Transform, With<MainCamera>>,
    projection: Single<&Projection, With<MainCamera>>,
    mut camera_state: Single<&mut FreeCameraState, With<MainCamera>>,
) {
    for cmd in reader.read() {
        let AppCommand::Camera(CameraCommand::Reset) = *cmd else {
            continue;
        };

        camera_state.enabled = false;

        let aspect = match &*projection {
            Projection::Perspective(p) => p.aspect_ratio,
            _ => window.aspect(),
        };

        **transform = frame_main_camera_transform(&window, aspect);
    }
}

fn on_toggle_free_camera(
    mut reader: MessageReader<AppCommand>,
    mut state: Single<&mut FreeCameraState, With<MainCamera>>,
) {
    for cmd in reader.read() {
        let AppCommand::Camera(CameraCommand::ToggleFreeCamera) = *cmd else {
            continue;
        };
        state.enabled = !state.enabled;
    }
}

#[cfg(target_os = "macos")]
fn apply_liquid_glass(
    _main_thread: NonSend<LiquidGlassMainThread>,
    query: Query<(Entity, &RawHandleWrapper), Added<Window>>,
) {
    for (entity, wrapper) in query.iter() {
        let ptr = match wrapper.get_window_handle() {
            RawWindowHandle::AppKit(h) => h.ns_view.as_ptr().cast::<std::ffi::c_void>(),
            _ => continue,
        };
        if ptr.is_null() {
            continue;
        }

        let manager = GlassViewManager::new();
        match manager.add_glass_view(ptr, GlassOptions::default()) {
            Ok(_) => info!("Liquid Glass successfully applied to window: {:?}", entity),
            Err(e) => bevy_log::error!("Window {:?} not ready for glass: {:?}", entity, e),
        }
    }
}

pub(crate) fn frame_main_camera_transform(window: &Window, aspect: f32) -> Transform {
    let m = window.meters();
    let center = Vec3::new(0.0, m.y * 0.5, 0.0);

    let half_fov_y = FOV_Y * 0.5;
    let tan_half_fov_y = half_fov_y.tan();

    let dist_to_fit_height = (m.y * 0.5) / tan_half_fov_y;
    let tan_half_fov_x = tan_half_fov_y * aspect;
    let dist_to_fit_width = (m.x * 0.5) / tan_half_fov_x;
    let dist = dist_to_fit_height.max(dist_to_fit_width);

    Transform::from_xyz(center.x, center.y, center.z + dist).looking_at(center, Vec3::Y)
}
