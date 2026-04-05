use std::path::PathBuf;

use bevy::camera::CameraUpdateSystems;
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy::window::{PrimaryWindow, Window as NativeWindow};
use bevy_cef::prelude::*;
use vmux_webview_app::JsEmitUiReadyPlugin;

use crate::layout::{Outline, OutlineMaterial, Tab};
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
            .add_systems(
                Update,
                (spawn_browser_on_new_tab, tick_outline_gradient_time),
            )
            .add_systems(
                PostUpdate,
                sync_browser_plane_to_window.after(CameraUpdateSystems),
            );
    }
}

const CAMERA_TO_PLANE: f32 = 3.0;
const OUTLINE_Z_BACK: f32 = 0.002;

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
    inner_px: Vec2,
    border_px: f32,
    r_px: f32,
    inner_world_half: Vec2,
    outer_world_half: Vec2,
}

impl BrowserPlaneLayout {
    fn webview_corner_uniform(&self) -> Vec4 {
        let w = self.inner_px.x.max(1.0e-6);
        let h = self.inner_px.y.max(1.0e-6);
        Vec4::new(self.r_px, w, h, 0.0)
    }
}

fn outline_material_for_plane(
    plane: &BrowserPlaneLayout,
    settings: &AppSettings,
    time_secs: f32,
) -> OutlineMaterial {
    let w_i = plane.inner_px.x.max(1.0e-6);
    let h_i = plane.inner_px.y.max(1.0e-6);
    let b = plane.border_px;
    let w_o = w_i + 2.0 * b;
    let h_o = h_i + 2.0 * b;
    let r_i = plane.r_px;
    let m_o = w_o.min(h_o);
    let r_o = (r_i + b).min(m_o * 0.5);
    let c = &settings.layout.pane.outline.color;
    let border_color = Color::srgb(c.r, c.g, c.b).to_linear().to_vec4();
    let g = &settings.layout.pane.outline.gradient;
    let accent = &g.accent;
    let border_accent = Color::srgb(accent.r, accent.g, accent.b).to_linear().to_vec4();
    let grad_on = if g.enabled { 1.0 } else { 0.0 };
    let gradient_params = Vec4::new(grad_on, g.speed, g.cycles.max(0.01), time_secs);
    let spread = settings.layout.pane.outline.glow.spread.max(0.5);
    let intensity = settings.layout.pane.outline.glow.intensity.max(0.0);
    let glow_on = if intensity > 1.0e-4 { 1.0 } else { 0.0 };
    OutlineMaterial {
        pane_inner: Vec4::new(r_i, w_i, h_i, 0.0),
        pane_outer: Vec4::new(r_o, w_o, h_o, 0.0),
        border_color,
        glow_params: Vec4::new(glow_on, intensity, spread, 0.0),
        gradient_params,
        border_accent,
        alpha_mode: AlphaMode::Blend,
    }
}

fn tick_outline_gradient_time(
    time: Res<Time>,
    mut materials: ResMut<Assets<OutlineMaterial>>,
    outlines: Query<&MeshMaterial3d<OutlineMaterial>, With<Outline>>,
) {
    let t = time.elapsed_secs();
    for mesh_mat in &outlines {
        let id = mesh_mat.id();
        let Some(m) = materials.get(id) else {
            continue;
        };
        if m.gradient_params.x <= 0.5 {
            continue;
        }
        let Some(m) = materials.get_mut(id) else {
            continue;
        };
        m.gradient_params.w = t;
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
    let inner_px = Vec2::new(
        (fw - 2.0 * inset).max(1.0e-6),
        (fh - 2.0 * inset).max(1.0e-6),
    );
    let w_i = inner_px.x;
    let h_i = inner_px.y;
    let border_px = settings.layout.pane.outline.width.max(0.0);
    let w_o = w_i + 2.0 * border_px;
    let h_o = h_i + 2.0 * border_px;
    let m = w_i.min(h_i);
    let r_px = settings.layout.pane.radius.min(m * 0.5).max(0.0);
    let base_half = camera_proj
        .single()
        .map(|(_, projection)| world_half_extents_fill_plane(projection, fw / fh, CAMERA_TO_PLANE))
        .unwrap_or_else(|_| Vec2::new(fw * 0.5, fh * 0.5));
    let inner_world_half = Vec2::new(base_half.x * w_i / fw, base_half.y * h_i / fh);
    let outer_world_half = Vec2::new(base_half.x * w_o / fw, base_half.y * h_o / fh);
    BrowserPlaneLayout {
        inner_px,
        border_px,
        r_px,
        inner_world_half,
        outer_world_half,
    }
}

fn spawn_browser_on_new_tab(
    mut commands: Commands,
    query: Query<Entity, Added<Tab>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut outline_materials: ResMut<Assets<OutlineMaterial>>,
    settings: Res<AppSettings>,
    time: Res<Time>,
    window: Query<&NativeWindow, With<PrimaryWindow>>,
    camera_proj: Query<(&Camera, &Projection), With<Camera3d>>,
    cameras: Query<&Camera>,
) {
    let plane = browser_plane_layout(&settings, &window, &camera_proj, &cameras);
    let time_secs = time.elapsed_secs();
    for tab in query.iter() {
        let mut mat = WebviewExtendStandardMaterial::default();
        mat.base.unlit = true;
        mat.base.alpha_mode = AlphaMode::Blend;
        mat.extension.pane_corner_clip = plane.webview_corner_uniform();
        let inner_mesh = meshes.add(Plane3d::new(Vec3::Z, plane.inner_world_half));
        let browser_id = commands
            .spawn((
                BrowserBundle {
                    browser: Browser,
                    source: WebviewSource::new(settings.browser.startup_url.as_str()),
                    mesh: Mesh3d(inner_mesh),
                    material: MeshMaterial3d(materials.add(mat)),
                },
                WebviewSize(plane.inner_px),
                BrowserVisualLayout {
                    inner_px: plane.inner_px,
                    border_px: plane.border_px,
                    r_px: plane.r_px,
                },
            ))
            .id();
        commands.entity(tab).add_child(browser_id);
        if plane.border_px > 0.0 {
            let outline_mat =
                outline_materials.add(outline_material_for_plane(&plane, &settings, time_secs));
            let outer_mesh = meshes.add(Plane3d::new(Vec3::Z, plane.outer_world_half));
            commands.entity(browser_id).with_children(|parent| {
                parent.spawn((
                    Outline,
                    Mesh3d(outer_mesh),
                    MeshMaterial3d(outline_mat),
                    Transform::from_translation(Vec3::new(0.0, 0.0, -OUTLINE_Z_BACK)),
                ));
            });
        }
    }
}

fn sync_browser_plane_to_window(
    mut commands: Commands,
    settings: Res<AppSettings>,
    time: Res<Time>,
    window: Query<&NativeWindow, With<PrimaryWindow>>,
    camera_proj: Query<(&Camera, &Projection), With<Camera3d>>,
    cameras: Query<&Camera>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut outline_materials: ResMut<Assets<OutlineMaterial>>,
    mut browsers: Query<
        (
            Entity,
            &mut WebviewSize,
            &mut Mesh3d,
            &MeshMaterial3d<WebviewExtendStandardMaterial>,
            &mut BrowserVisualLayout,
            &Children,
        ),
        (With<Browser>, Without<Outline>),
    >,
    outline_q: Query<
        (Entity, &Mesh3d, &MeshMaterial3d<OutlineMaterial>),
        With<Outline>,
    >,
) {
    let plane = browser_plane_layout(&settings, &window, &camera_proj, &cameras);
    let time_secs = time.elapsed_secs();
    let stamp = BrowserVisualLayout {
        inner_px: plane.inner_px,
        border_px: plane.border_px,
        r_px: plane.r_px,
    };
    for (browser_entity, mut webview_size, mesh_3d, mat_h, mut visual, children) in browsers.iter_mut()
    {
        if *visual == stamp {
            continue;
        }
        *visual = stamp;
        webview_size.0 = plane.inner_px;
        if let Some(mesh) = meshes.get_mut(&mesh_3d.0) {
            *mesh = Mesh::from(Plane3d::new(Vec3::Z, plane.inner_world_half));
        }
        if let Some(mat) = materials.get_mut(&mat_h.0) {
            mat.base.unlit = true;
            mat.extension.pane_corner_clip = plane.webview_corner_uniform();
        }

        let mut outline_entity: Option<Entity> = None;
        for child in children.iter() {
            if outline_q.contains(child) {
                outline_entity = Some(child);
                break;
            }
        }

        if plane.border_px > 0.0 {
            let mat = outline_material_for_plane(&plane, &settings, time_secs);
            if let Some(oe) = outline_entity {
                if let Ok((_, om, mat_handle)) = outline_q.get(oe) {
                    if let Some(m) = meshes.get_mut(&om.0) {
                        *m = Mesh::from(Plane3d::new(Vec3::Z, plane.outer_world_half));
                    }
                    if let Some(pm) = outline_materials.get_mut(&mat_handle.0) {
                        *pm = mat;
                    }
                }
            } else {
                let outline_mat = outline_materials.add(mat);
                let outer_mesh = meshes.add(Plane3d::new(Vec3::Z, plane.outer_world_half));
                commands.entity(browser_entity).with_children(|parent| {
                    parent.spawn((
                        Outline,
                        Mesh3d(outer_mesh),
                        MeshMaterial3d(outline_mat),
                        Transform::from_translation(Vec3::new(0.0, 0.0, -OUTLINE_Z_BACK)),
                    ));
                });
            }
        } else if let Some(oe) = outline_entity {
            commands.entity(oe).despawn();
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
