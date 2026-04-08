use crate::command::{AppCommand, CameraCommand};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::{
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin, FreeCameraState},
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
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

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Spawn3dCamera;

const DISPLAY_HALF_HEIGHT: f32 = 1.0;
const DISPLAY_DEPTH: f32 = 0.02;
const DISPLAY_CENTER: Vec3 = Vec3::new(0.0, 1.0, 0.0);
const FOV_Y: f32 = std::f32::consts::FRAC_PI_4;
const DISPLAY_FRONT_FACE_Z: f32 = 0.5 * DISPLAY_DEPTH;
const BOUNCE_DISPLAY_CLEARANCE: f32 = 0.9;

#[derive(Component)]
struct DisplayPanel;

#[derive(Component)]
struct MainCamera;

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
            .configure_sets(Startup, Spawn3dCamera)
            .add_systems(Startup, (setup).chain().in_set(Spawn3dCamera))
            .add_systems(Update, fit_to_window_on_resize)
            .add_observer(on_reset_camera)
            .add_observer(on_toggle_free_camera);

        #[cfg(target_os = "macos")]
        app.insert_resource(ClearColor(Color::NONE))
            .insert_non_send_resource(LiquidGlassMainThread::default())
            .add_systems(Update, apply_liquid_glass);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut webview_materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    window_q: Query<&Window, With<PrimaryWindow>>,
) {
    if let Ok(window) = window_q.single() {
        let (scale, dist) = get_camera_distance(&window);

        commands.spawn(InfiniteGridBundle::default());
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(0.35))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.35, 0.38, 0.45),
                emissive: Color::srgb(1.0, 0.45, 0.05).into(),
                emissive_exposure_weight: 0.0,
                ..default()
            })),
            Transform::from_xyz(
                DISPLAY_CENTER.x,
                DISPLAY_CENTER.y,
                DISPLAY_CENTER.z - 4.0 - 0.5 * DISPLAY_DEPTH,
            ),
        ));

        let display_panel = commands
            .spawn((
                DisplayPanel,
                Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(20.0, 20.0, 20.0, 0.7),
                    alpha_mode: AlphaMode::Blend,
                    perceptual_roughness: 0.12,
                    metallic: 0.0,
                    specular_transmission: 0.9,
                    diffuse_transmission: 1.0,
                    thickness: 0.1,
                    ior: 1.5,
                    ..default()
                })),
                Transform::from_translation(DISPLAY_CENTER).with_scale(scale),
            ))
            .id();

        commands.spawn((
            WebviewSource::new("https://github.com/not-elm/bevy_cef"),
            Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            MeshMaterial3d(webview_materials.add(WebviewExtendStandardMaterial {
                base: StandardMaterial {
                    unlit: true,
                    ..default()
                },
                ..default()
            })),
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.5 + 1e-3)),
            ChildOf(display_panel),
        ));

        commands.spawn((
            MainCamera,
            Camera3d::default(),
            Transform::from_xyz(0.0, 1.0, dist + DISPLAY_FRONT_FACE_Z)
                .looking_at(DISPLAY_CENTER, Vec3::Y),
            FreeCamera {
                sensitivity: 0.2,
                friction: 25.0,
                walk_speed: 0.5,
                run_speed: 2.5,
                ..default()
            },
            Bloom::NATURAL,
        ));

        let material_emissive1 = materials.add(StandardMaterial {
            emissive: LinearRgba::rgb(0.0, 0.0, 150.0), // 3. Put something bright in a dark environment to see the effect
            ..default()
        });
        let material_emissive2 = materials.add(StandardMaterial {
            emissive: LinearRgba::rgb(1000.0, 1000.0, 1000.0),
            ..default()
        });
        let material_emissive3 = materials.add(StandardMaterial {
            emissive: LinearRgba::rgb(50.0, 0.0, 0.0),
            ..default()
        });
        let material_non_emissive = materials.add(StandardMaterial {
            base_color: Color::BLACK,
            ..default()
        });

        let bounce_mesh = meshes.add(Sphere::new(0.35));

        let camera_pos = Vec3::new(0.0, 1.0, dist + DISPLAY_FRONT_FACE_Z);
        let clear_radius_sq = camera_pos.distance_squared(DISPLAY_CENTER);

        for x in -5..5 {
            for z in -5..5 {
                let px = x as f32 * 2.0;
                let pz = z as f32 * 2.0;
                let p = Vec3::new(px, 0.0, pz);
                if (p - camera_pos).length_squared() < clear_radius_sq {
                    continue;
                }

                let half_x = scale.x * 0.5;
                let half_z = scale.z * 0.5;
                let cx = px.abs() - half_x;
                let cz = pz.abs() - half_z;
                if cx <= 0.0 && cz <= 0.0 {
                    continue;
                }
                let ox = cx.max(0.0);
                let oz = cz.max(0.0);
                if ox * ox + oz * oz < BOUNCE_DISPLAY_CLEARANCE * BOUNCE_DISPLAY_CLEARANCE {
                    continue;
                }

                // This generates a pseudo-random integer between `[0, 6)`, but deterministically so
                // the same spheres are always the same colors.
                let mut hasher = DefaultHasher::new();
                (x, z).hash(&mut hasher);
                let rand = (hasher.finish() + 3) % 6;

                let (material, scale) = match rand {
                    0 => (material_emissive1.clone(), 0.5),
                    1 => (material_emissive2.clone(), 0.1),
                    2 => (material_emissive3.clone(), 1.0),
                    3..=5 => (material_non_emissive.clone(), 1.5),
                    _ => unreachable!(),
                };

                commands.spawn((
                    Mesh3d(bounce_mesh.clone()),
                    MeshMaterial3d(material),
                    Transform::from_xyz(px, 0.0, pz).with_scale(Vec3::splat(scale)),
                    Bouncing,
                ));
            }
        }
    }
}

fn get_camera_distance(window: &Window) -> (Vec3, f32) {
    let width = window.width().max(1.0);
    let height = window.height().max(1.0);
    let aspect = width / height;
    let display_height = 2.0 * DISPLAY_HALF_HEIGHT;
    let display_width = display_height * aspect;
    let dist = DISPLAY_HALF_HEIGHT / (FOV_Y * 0.5).tan();
    (
        Vec3::new(display_width, display_height, DISPLAY_DEPTH),
        dist,
    )
}

fn fit_to_window_on_resize(
    window: Single<&Window, With<PrimaryWindow>>,
    mut last_px: Local<Option<(f32, f32)>>,
    mut display: Single<&mut Transform, (With<DisplayPanel>, Without<MainCamera>)>,
) {
    let w = window.width();
    let h = window.height();
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    if last_px.is_some_and(|p| (p.0 - w).abs() < 0.5 && (p.1 - h).abs() < 0.5) {
        return;
    }
    *last_px = Some((w, h));

    let (scale, _) = get_camera_distance(&window);
    display.translation = DISPLAY_CENTER;
    display.scale = scale;
}

fn on_reset_camera(
    trigger: On<AppCommand>,
    mut camera: Single<&mut Transform, With<MainCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    let AppCommand::Camera(CameraCommand::Reset) = *trigger.event() else {
        return;
    };
    let (_, dist) = get_camera_distance(&window);
    **camera = Transform::from_xyz(
        DISPLAY_CENTER.x,
        DISPLAY_CENTER.y,
        dist + DISPLAY_FRONT_FACE_Z,
    )
    .looking_at(DISPLAY_CENTER, Vec3::Y);
}

fn on_toggle_free_camera(
    trigger: On<AppCommand>,
    mut state: Single<&mut FreeCameraState, With<MainCamera>>,
) {
    let AppCommand::Camera(CameraCommand::ToggleFreeCamera) = *trigger.event() else {
        return;
    };
    state.enabled = !state.enabled;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins(ScenePlugin);
    }
}
