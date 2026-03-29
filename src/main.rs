//! vmux: Bevy + bevy_cef — webview on a 3D plane.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::camera::Camera;
use bevy::render::camera::camera_system;
use bevy::render::view::screenshot::{Capturing, Screenshot, save_to_disk};
use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};
use bevy_cef::prelude::*;
use leafwing_input_manager::prelude::*;

const WEBVIEW_URL: &str = "https://github.com/not-elm/bevy_cef";
const CAMERA_DISTANCE: f32 = 3.0;

/// Chromium page zoom via CEF [`SetZoomLevel`](https://github.com/not-elm/bevy_cef/blob/main/examples/zoom_level.rs).
/// `0.0` matches typical desktop browsers (e.g. Arc at 100%).
const CEF_PAGE_ZOOM_LEVEL: f64 = 0.0;

/// Marker on the webview entity (queries / multi-webview safety — see [bevy_cef concepts](https://not-elm.github.io/bevy_cef/concepts)).
#[derive(Component)]
struct VmuxWebview;

#[derive(Component)]
struct VmuxWorldCamera;

#[derive(Actionlike, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AppAction {
    Quit,
}

#[derive(Component)]
struct AppInputRoot;

fn cef_root_cache_path() -> Option<String> {
    if let Ok(home) = std::env::var("HOME") {
        #[cfg(target_os = "macos")]
        {
            return Some(format!("{home}/Library/Caches/vmux/cef"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Some(format!("{home}/.cache/vmux/cef"));
        }
    }
    std::env::temp_dir()
        .to_str()
        .map(|p| format!("{p}/vmux_cef"))
}

fn main() {
    #[cfg(not(target_os = "macos"))]
    bevy_cef::prelude::early_exit_if_subprocess();

    let cef_plugin = CefPlugin {
        command_line_config: CommandLineConfig {
            switches: vec![],
            switch_values: vec![],
        },
        root_cache_path: cef_root_cache_path(),
        ..Default::default()
    };

    App::new()
        .add_plugins((
            DefaultPlugins,
            InputManagerPlugin::<AppAction>::default(),
            cef_plugin,
        ))
        .add_systems(
            Startup,
            (
                spawn_camera,
                spawn_directional_light,
                spawn_webview,
                spawn_app_input,
            ),
        )
        .add_systems(
            Update,
            (
                exit_on_quit_action,
                screenshot_on_spacebar,
                screenshot_saving_cursor,
            ),
        )
        .add_systems(
            PostUpdate,
            (
                sync_webview_layout_size_to_window,
                fit_webview_plane_to_window,
            )
                .chain()
                .after(camera_system),
        )
        .run();
}

fn spawn_app_input(mut commands: Commands) {
    let mut input_map = InputMap::<AppAction>::default();
    input_map.insert(
        AppAction::Quit,
        ButtonlikeChord::modified(ModifierKey::Super, KeyCode::KeyQ),
    );
    commands.spawn((AppInputRoot, input_map, ActionState::<AppAction>::default()));
}

fn exit_on_quit_action(
    query: Query<&ActionState<AppAction>, With<AppInputRoot>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let Ok(state) = query.single() else {
        return;
    };
    if state.just_pressed(&AppAction::Quit) {
        app_exit.write(AppExit::Success);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        VmuxWorldCamera,
        Camera3d::default(),
        Transform::from_translation(Vec3::new(0., 0., CAMERA_DISTANCE))
            .looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_directional_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_translation(Vec3::new(1., 1., 1.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_webview(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    commands.spawn((
        VmuxWebview,
        WebviewSource::new(WEBVIEW_URL),
        // `WebviewSize` default is overwritten in `sync_webview_layout_size_to_window`.
        ZoomLevel(CEF_PAGE_ZOOM_LEVEL),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE))),
        MeshMaterial3d(materials.add(WebviewExtendStandardMaterial {
            base: StandardMaterial {
                // `WebviewExtendStandardMaterial` uses the PBR path by default; scene lights then
                // shade the plane (e.g. brighter toward the directional light). Unlit = flat screen.
                unlit: true,
                ..default()
            },
            extension: WebviewMaterial::default(),
        })),
    ));
}

/// Sets CEF **layout viewport** (`WebviewSize`) to match what the 3D camera actually renders (logical
/// viewport), falling back to the primary window’s inner size when the camera is not ready yet.
///
/// Runs **after** [`camera_system`] so [`Camera::logical_viewport_size`] is current — this keeps
/// `WebviewSize`, pointer → CEF coordinates, and [`fit_webview_plane_to_window`] aligned after
/// resize and on HiDPI.
///
/// Using **physical** pixels for `WebviewSize` would make HiDPI windows look like an ultra‑wide
/// viewport (e.g. GitHub’s fixed max‑width column with huge gutters). Browsers use logical
/// `innerWidth` / `innerHeight`; matching that reproduces normal layout.
fn sync_webview_layout_size_to_window(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<&Camera, With<VmuxWorldCamera>>,
    mut webview: Query<&mut WebviewSize, With<VmuxWebview>>,
) {
    let Ok(mut webview_size) = webview.single_mut() else {
        return;
    };

    let from_camera = camera.single().ok().and_then(|cam| {
        let sz = cam.logical_viewport_size()?;
        (sz.x > 0.0 && sz.y > 0.0 && sz.x.is_finite() && sz.y.is_finite()).then_some(sz)
    });

    let next = if let Some(sz) = from_camera {
        sz
    } else if let Ok(window) = window.single() {
        let w = window.width().max(1.0e-3);
        let h = window.height().max(1.0e-3);
        if !(w.is_finite() && h.is_finite()) {
            return;
        }
        Vec2::new(w, h)
    } else {
        return;
    };

    if webview_size.0 != next {
        webview_size.0 = next;
    }
}

/// Save a PNG of the primary window (see [Bevy screenshot example](https://bevy.org/examples/window/screenshot/)).
fn screenshot_on_spacebar(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    mut counter: Local<u32>,
) {
    if input.just_pressed(KeyCode::Space) {
        let path = format!("./vmux-screenshot-{}.png", *counter);
        *counter += 1;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
}

fn screenshot_saving_cursor(
    mut commands: Commands,
    screenshot_saving: Query<Entity, With<Capturing>>,
    window: Single<Entity, With<Window>>,
) {
    match screenshot_saving.iter().count() {
        0 => {
            commands.entity(*window).remove::<CursorIcon>();
        }
        x if x > 0 => {
            commands
                .entity(*window)
                .insert(CursorIcon::from(SystemCursorIcon::Progress));
        }
        _ => {}
    }
}

fn fit_webview_plane_to_window(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &Projection), (With<Camera3d>, With<VmuxWorldCamera>)>,
    mut webview: Query<&mut Transform, With<VmuxWebview>>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let Ok((camera, projection)) = camera.single() else {
        return;
    };
    let Ok(mut transform) = webview.single_mut() else {
        return;
    };

    let Projection::Perspective(perspective) = projection else {
        return;
    };

    let w = window.width();
    let h = window.height();
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
        return;
    }

    let aspect = camera
        .logical_viewport_size()
        .filter(|s| s.x > 0.0 && s.y > 0.0 && s.x.is_finite() && s.y.is_finite())
        .map(|s| s.x / s.y)
        .unwrap_or(w / h);

    let tan_half_fov = (perspective.fov * 0.5).tan();
    let half_h = CAMERA_DISTANCE * tan_half_fov;
    let half_w = half_h * aspect;
    let new_scale = Vec3::new(half_w, half_h, 1.0);
    if (transform.scale.x - new_scale.x).abs() > 1e-4
        || (transform.scale.y - new_scale.y).abs() > 1e-4
    {
        transform.scale = new_scale;
    }
}
