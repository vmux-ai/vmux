use std::path::PathBuf;

use bevy::camera::CameraUpdateSystems;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, Window as NativeWindow};
use bevy_cef::prelude::*;
use vmux_webview_app::JsEmitUiReadyPlugin;

use crate::layout::Tab;
use crate::settings::AppSettings;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JsEmitUiReadyPlugin,
            CefPlugin {
                root_cache_path: example_cef_root_cache_path(),
                ..default()
            },
        ))
        .add_systems(Update, spawn_browser_on_new_tab)
        .add_systems(
            PostUpdate,
            sync_browser_plane_to_window.after(CameraUpdateSystems),
        );
    }
}

const CAMERA_TO_PLANE: f32 = 3.0;
const PANE_CORNER_CLIP: f32 = 0.0;

#[derive(Bundle)]
struct BrowserBundle {
    browser: Browser,
    source: WebviewSource,
    mesh: Mesh3d,
    material: MeshMaterial3d<WebviewExtendStandardMaterial>,
}

#[derive(Component)]
struct Browser;

pub struct BrowserPlugin;

struct BrowserPlaneLayout {
    layout_px: Vec2,
    r_px: f32,
    world_half: Vec2,
}

fn browser_plane_layout(
    settings: &AppSettings,
    window: &Query<&NativeWindow, With<PrimaryWindow>>,
    camera_proj: &Query<(&Camera, &Projection), With<Camera3d>>,
    cameras: &Query<&Camera>,
) -> BrowserPlaneLayout {
    let layout_px = browser_layout_px(window, cameras);
    let w = layout_px.x.max(1.0e-6);
    let h = layout_px.y.max(1.0e-6);
    let m = w.min(h);
    let r_px = settings.layout.pane.border_radius.min(m * 0.5).max(0.0);
    let world_half = camera_proj
        .single()
        .map(|(_, projection)| world_half_extents_fill_plane(projection, w / h, CAMERA_TO_PLANE))
        .unwrap_or_else(|_| Vec2::new(w * 0.5, h * 0.5));
    BrowserPlaneLayout {
        layout_px,
        r_px,
        world_half,
    }
}

fn spawn_browser_on_new_tab(
    mut commands: Commands,
    query: Query<Entity, Added<Tab>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: Res<AppSettings>,
    window: Query<&NativeWindow, With<PrimaryWindow>>,
    camera_proj: Query<(&Camera, &Projection), With<Camera3d>>,
    cameras: Query<&Camera>,
) {
    let plane = browser_plane_layout(&settings, &window, &camera_proj, &cameras);
    let w = plane.layout_px.x.max(1.0e-6);
    let h = plane.layout_px.y.max(1.0e-6);
    for tab in query.iter() {
        let mut mat = WebviewExtendStandardMaterial::default();
        mat.extension.pane_corner_clip = Vec4::new(plane.r_px, w, h, PANE_CORNER_CLIP);
        commands.entity(tab).with_children(|parent| {
            parent.spawn((
                BrowserBundle {
                    browser: Browser,
                    source: WebviewSource::new(settings.browser.startup_url.as_str()),
                    mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, plane.world_half))),
                    material: MeshMaterial3d(materials.add(mat)),
                },
                WebviewSize(plane.layout_px),
            ));
        });
    }
}

fn sync_browser_plane_to_window(
    settings: Res<AppSettings>,
    window: Query<&NativeWindow, With<PrimaryWindow>>,
    camera_proj: Query<(&Camera, &Projection), With<Camera3d>>,
    cameras: Query<&Camera>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut browsers: Query<
        (
            &mut WebviewSize,
            &mut Mesh3d,
            &MeshMaterial3d<WebviewExtendStandardMaterial>,
        ),
        With<Browser>,
    >,
) {
    let plane = browser_plane_layout(&settings, &window, &camera_proj, &cameras);
    let w = plane.layout_px.x.max(1.0e-6);
    let h = plane.layout_px.y.max(1.0e-6);
    for (mut webview_size, mesh_3d, mat_h) in browsers.iter_mut() {
        let prev = webview_size.0;
        if (prev.x - plane.layout_px.x).abs() < 0.5 && (prev.y - plane.layout_px.y).abs() < 0.5 {
            continue;
        }
        webview_size.0 = plane.layout_px;
        if let Some(mesh) = meshes.get_mut(&mesh_3d.0) {
            *mesh = Mesh::from(Plane3d::new(Vec3::Z, plane.world_half));
        }
        if let Some(mat) = materials.get_mut(&mat_h.0) {
            mat.extension.pane_corner_clip = Vec4::new(plane.r_px, w, h, PANE_CORNER_CLIP);
        }
    }
}

fn browser_layout_px(
    window: &Query<&NativeWindow, With<PrimaryWindow>>,
    cameras: &Query<&Camera>,
) -> Vec2 {
    if let Ok(w) = window.single() {
        let width = w.width();
        let height = w.height();
        if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
            return Vec2::new(width, height);
        }
    }
    for cam in cameras.iter() {
        if let Some(size) = cam.logical_viewport_size()
            && size.x > 0.0
            && size.y > 0.0
            && size.x.is_finite()
            && size.y.is_finite()
        {
            return size;
        }
    }
    WebviewSize::default().0
}

fn world_half_extents_fill_plane(
    projection: &Projection,
    window_aspect: f32,
    camera_to_plane: f32,
) -> Vec2 {
    let aspect = if window_aspect.is_finite() && window_aspect > 0.0 {
        window_aspect
    } else {
        1.0
    };
    match projection {
        Projection::Perspective(p) => {
            let half_h = camera_to_plane * (p.fov * 0.5).tan();
            let half_w = half_h * aspect;
            Vec2::new(half_w, half_h)
        }
        _ => Vec2::new(camera_to_plane * 0.5 * aspect, camera_to_plane * 0.5),
    }
}

fn example_cef_root_cache_path() -> Option<String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            let base = if cfg!(target_os = "macos") {
                home.join("Library/Caches/vmux_examples")
            } else {
                home.join(".cache/vmux_examples")
            };
            base.join("cef").to_string_lossy().into_owned()
        })
        .or_else(|| {
            std::env::temp_dir()
                .to_str()
                .map(|p| format!("{p}/vmux_examples_cef"))
        })
}
