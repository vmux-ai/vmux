use std::path::PathBuf;

use bevy::camera::CameraUpdateSystems;
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
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

#[derive(Bundle)]
struct BrowserBundle {
    browser: Browser,
    source: WebviewSource,
    mesh: Mesh3d,
    material: MeshMaterial3d<WebviewExtendStandardMaterial>,
}

#[derive(Component)]
struct Browser;

#[derive(Component, Clone, Copy, PartialEq)]
struct BrowserVisualLayout {
    inner_px: Vec2,
    border_px: f32,
    r_px: f32,
}

pub struct BrowserPlugin;

struct BrowserPlaneLayout {
    layout_px: Vec2,
    border_px: f32,
    r_px: f32,
    world_half: Vec2,
}

impl BrowserPlaneLayout {
    fn webview_corner_uniform(&self) -> Vec4 {
        let w = self.layout_px.x.max(1.0e-6);
        let h = self.layout_px.y.max(1.0e-6);
        Vec4::new(self.r_px, w, h, 0.0)
    }

    fn vmux_border_uniforms(&self) -> (Vec4, Vec4) {
        let b = self.border_px;
        if b <= 0.0 {
            return (Vec4::ZERO, Vec4::ZERO);
        }
        let w_i = self.layout_px.x.max(1.0e-6);
        let h_i = self.layout_px.y.max(1.0e-6);
        let w_o = w_i + 2.0 * b;
        let h_o = h_i + 2.0 * b;
        let params = Vec4::new(1.0, b, w_o, h_o);
        let color = Color::srgb(0.52, 0.52, 0.58).to_linear().to_vec4();
        (params, color)
    }
}

fn browser_plane_layout(
    settings: &AppSettings,
    window: &Query<&NativeWindow, With<PrimaryWindow>>,
    camera_proj: &Query<(&Camera, &Projection), With<Camera3d>>,
    cameras: &Query<&Camera>,
) -> BrowserPlaneLayout {
    let full_px = browser_window_px(window, cameras);
    let fw = full_px.x.max(1.0e-6);
    let fh = full_px.y.max(1.0e-6);
    let inset = settings.layout.window.padding.max(0.0);
    let layout_px = Vec2::new(
        (fw - 2.0 * inset).max(1.0e-6),
        (fh - 2.0 * inset).max(1.0e-6),
    );
    let w_i = layout_px.x;
    let h_i = layout_px.y;
    let border_px = settings.layout.pane.border.max(0.0);
    let w_o = w_i + 2.0 * border_px;
    let h_o = h_i + 2.0 * border_px;
    let m = w_i.min(h_i);
    let r_px = settings.layout.pane.radius.min(m * 0.5).max(0.0);
    let mut world_half = camera_proj
        .single()
        .map(|(_, projection)| world_half_extents_fill_plane(projection, fw / fh, CAMERA_TO_PLANE))
        .unwrap_or_else(|_| Vec2::new(fw * 0.5, fh * 0.5));
    world_half.x *= w_o / fw;
    world_half.y *= h_o / fh;
    BrowserPlaneLayout {
        layout_px,
        border_px,
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
    for tab in query.iter() {
        let mut mat = WebviewExtendStandardMaterial::default();
        mat.base.unlit = true;
        mat.base.alpha_mode = AlphaMode::Blend;
        mat.extension.pane_corner_clip = plane.webview_corner_uniform();
        let (bp, bc) = plane.vmux_border_uniforms();
        mat.extension.vmux_border_params = bp;
        mat.extension.vmux_border_color = bc;
        commands.entity(tab).with_children(|parent| {
            parent.spawn((
                BrowserBundle {
                    browser: Browser,
                    source: WebviewSource::new(settings.browser.startup_url.as_str()),
                    mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, plane.world_half))),
                    material: MeshMaterial3d(materials.add(mat)),
                },
                WebviewSize(plane.layout_px),
                BrowserVisualLayout {
                    inner_px: plane.layout_px,
                    border_px: plane.border_px,
                    r_px: plane.r_px,
                },
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
            &mut BrowserVisualLayout,
        ),
        With<Browser>,
    >,
) {
    let plane = browser_plane_layout(&settings, &window, &camera_proj, &cameras);
    let stamp = BrowserVisualLayout {
        inner_px: plane.layout_px,
        border_px: plane.border_px,
        r_px: plane.r_px,
    };
    for (mut webview_size, mesh_3d, mat_h, mut visual) in browsers.iter_mut() {
        if *visual == stamp {
            continue;
        }
        *visual = stamp;
        webview_size.0 = plane.layout_px;
        if let Some(mesh) = meshes.get_mut(&mesh_3d.0) {
            *mesh = Mesh::from(Plane3d::new(Vec3::Z, plane.world_half));
        }
        if let Some(mat) = materials.get_mut(&mat_h.0) {
            mat.base.unlit = true;
            mat.extension.pane_corner_clip = plane.webview_corner_uniform();
            let (bp, bc) = plane.vmux_border_uniforms();
            mat.extension.vmux_border_params = bp;
            mat.extension.vmux_border_color = bc;
        }
    }
}

fn browser_window_px(
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
