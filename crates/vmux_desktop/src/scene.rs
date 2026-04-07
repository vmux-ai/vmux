use crate::command::{AppCommand, CameraCommand};
use bevy::prelude::*;
use bevy::{
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    window::PrimaryWindow,
};
use bevy_infinite_grid::{InfiniteGridBundle, InfiniteGridPlugin};

// #[cfg(target_os = "macos")]
// use bevy::window::RawHandleWrapper;
// #[cfg(target_os = "macos")]
// use liquid_glass_rs::{GlassOptions, GlassViewManager};
// #[cfg(target_os = "macos")]
// use raw_window_handle::RawWindowHandle;
// #[cfg(target_os = "macos")]
// use std::marker::PhantomData;
// #[cfg(target_os = "macos")]
// use std::rc::Rc;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Spawn3dCamera;

const DISPLAY_HALF_HEIGHT: f32 = 0.5;
const DISPLAY_DEPTH: f32 = 0.02;
const DISPLAY_CENTER: Vec3 = Vec3::new(0.0, 1.0, 0.0);
const FOV_Y: f32 = std::f32::consts::FRAC_PI_4;

#[derive(Component)]
struct DisplayPanel;

#[derive(Component)]
struct MainCamera;

#[derive(Default)]
pub struct ScenePlugin;

// #[cfg(target_os = "macos")]
// struct LiquidGlassMainThread(PhantomData<Rc<()>>);

// #[cfg(target_os = "macos")]
// impl Default for LiquidGlassMainThread {
//     fn default() -> Self {
//         Self(PhantomData)
//     }
// }

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FreeCameraPlugin, InfiniteGridPlugin))
            .insert_resource(ClearColor(Color::BLACK))
            .configure_sets(Startup, Spawn3dCamera)
            .add_systems(Startup, (setup).chain().in_set(Spawn3dCamera))
            .add_systems(Update, fit_to_window_on_resize)
            .add_observer(on_reset_camera);

        // #[cfg(target_os = "macos")]
        // app.insert_resource(ClearColor(Color::NONE))
        //     .insert_non_send_resource(LiquidGlassMainThread::default())
        //     .add_systems(Update, apply_liquid_glass);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    window_q: Query<&Window, With<PrimaryWindow>>,
) {
    if let Ok(window) = window_q.single() {
        let (scale, dist) = display_scale_and_camera_dist(window);

        commands.spawn(InfiniteGridBundle::default());

        commands.spawn((
            DisplayPanel,
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                emissive: LinearRgba::WHITE,
                emissive_exposure_weight: 0.0,
                ..default()
            })),
            Transform::from_translation(DISPLAY_CENTER).with_scale(scale),
        ));

        commands.spawn((
            MainCamera,
            Camera3d::default(),
            Transform::from_xyz(0.0, 1.0, dist).looking_at(DISPLAY_CENTER, Vec3::Y),
            FreeCamera {
                sensitivity: 0.2,
                friction: 25.0,
                walk_speed: 0.5,
                run_speed: 3.0,
                ..default()
            },
        ));
    }
}

fn display_scale_and_camera_dist(window: &Window) -> (Vec3, f32) {
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

    let (scale, _) = display_scale_and_camera_dist(&window);
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
    let (_, dist) = display_scale_and_camera_dist(&window);
    **camera = Transform::from_xyz(DISPLAY_CENTER.x, DISPLAY_CENTER.y, dist)
        .looking_at(DISPLAY_CENTER, Vec3::Y);
}

// #[cfg(target_os = "macos")]
// fn apply_liquid_glass(
//     _main_thread: NonSend<LiquidGlassMainThread>,
//     query: Query<(Entity, &RawHandleWrapper), Added<Window>>,
// ) {
//     for (entity, wrapper) in query.iter() {
//         let ptr = match wrapper.get_window_handle() {
//             RawWindowHandle::AppKit(h) => h.ns_view.as_ptr().cast::<std::ffi::c_void>(),
//             _ => continue,
//         };
//         if ptr.is_null() {
//             continue;
//         }

//         let manager = GlassViewManager::new();
//         match manager.add_glass_view(ptr, GlassOptions::default()) {
//             Ok(_) => info!("Liquid Glass successfully applied to window: {:?}", entity),
//             Err(e) => bevy_log::error!("Window {:?} not ready for glass: {:?}", entity, e),
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins(ScenePlugin);
    }
}
