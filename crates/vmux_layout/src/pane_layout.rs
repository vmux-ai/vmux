//! World-space pane rectangles, [`WebviewSize`], depth bias, and CEF resize sync.

use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::Browsers;

use crate::{CAMERA_DISTANCE, LayoutTree, Pane, PixelRect, Root, VmuxWorldCamera, solve_layout};

/// World-space Z separation between pane planes (camera at +Z; larger Z is closer to the camera).
const PANE_Z_STRIDE: f32 = 0.05;

/// Per-pane [`StandardMaterial::depth_bias`] step so stacked panes win the depth test (see [`apply_pane_layout`]).
const PANE_DEPTH_BIAS_STRIDE: f32 = 250.0;

/// Upper bound on CEF OSR backing size (longest side in layout pixels). Uncapped sizes track the
/// full window/pane pixel area and are very expensive for typing/compositing; the mesh still fills
/// the pane — the texture is upscaled slightly when capped.
const MAX_CEF_BACKING_LONG_SIDE: f32 = 1536.0;

#[inline]
fn clamp_webview_backing_size(layout_px: Vec2) -> Vec2 {
    let w = layout_px.x.max(1.0);
    let h = layout_px.y.max(1.0);
    let m = w.max(h);
    if m <= MAX_CEF_BACKING_LONG_SIDE {
        return Vec2::new(w, h);
    }
    let s = MAX_CEF_BACKING_LONG_SIDE / m;
    Vec2::new((w * s).max(1.0), (h * s).max(1.0))
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn apply_pane_layout(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &Projection), (With<Camera3d>, With<VmuxWorldCamera>)>,
    layout_q: Query<&LayoutTree, With<Root>>,
    panes: Query<Entity, With<Pane>>,
    mut transforms: Query<&mut Transform, With<Pane>>,
    mut sizes: Query<&mut WebviewSize, With<Pane>>,
    mesh_mat: Query<&MeshMaterial3d<WebviewExtendStandardMaterial>, With<Pane>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let Ok((camera, projection)) = camera.single() else {
        return;
    };
    let Ok(layout) = layout_q.single() else {
        return;
    };

    let Projection::Perspective(perspective) = projection else {
        return;
    };

    let vw = window.width();
    let vh = window.height();
    if !(vw.is_finite() && vh.is_finite()) || vw <= 0.0 || vh <= 0.0 {
        return;
    }

    let aspect = camera
        .logical_viewport_size()
        .filter(|s| s.x > 0.0 && s.y > 0.0 && s.x.is_finite() && s.y.is_finite())
        .map(|s| s.x / s.y)
        .unwrap_or(vw / vh);

    let tan_half_fov = (perspective.fov * 0.5).tan();
    let half_h = CAMERA_DISTANCE * tan_half_fov;
    let half_w = half_h * aspect;

    let entity_alive = |e: Entity| panes.contains(e);
    let area = PixelRect {
        x: 0.0,
        y: 0.0,
        w: vw,
        h: vh,
    };
    let mut rects = solve_layout(&layout.root, area, entity_alive);
    rects.sort_by(|a, b| {
        let cy_a = a.1.y + a.1.h * 0.5;
        let cy_b = b.1.y + b.1.h * 0.5;
        cy_b.partial_cmp(&cy_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let cx_a = a.1.x + a.1.w * 0.5;
                let cx_b = b.1.x + b.1.w * 0.5;
                cx_b.partial_cmp(&cx_a).unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut z_eps = 0.0_f32;
    for (i, (entity, pr)) in rects.into_iter().enumerate() {
        let Ok(mut tf) = transforms.get_mut(entity) else {
            continue;
        };
        let Ok(mut ws) = sizes.get_mut(entity) else {
            continue;
        };

        let cx = pr.x + pr.w * 0.5;
        let cy = pr.y + pr.h * 0.5;
        let nx = cx / vw;
        let ny = cy / vh;

        let wx = (nx - 0.5) * 2.0 * half_w;
        let wy = (0.5 - ny) * 2.0 * half_h;

        let scale_x = (pr.w / vw) * half_w;
        let scale_y = (pr.h / vh) * half_h;

        tf.translation = Vec3::new(wx, wy, z_eps);
        tf.scale = Vec3::new(scale_x.max(1.0e-4), scale_y.max(1.0e-4), 1.0);
        ws.0 = clamp_webview_backing_size(Vec2::new(pr.w.max(1.0), pr.h.max(1.0)));
        z_eps += PANE_Z_STRIDE;

        if let Ok(handle) = mesh_mat.get(entity)
            && let Some(mat) = materials.get_mut(handle.id())
        {
            mat.base.depth_bias = i as f32 * PANE_DEPTH_BIAS_STRIDE;
        }
    }
}

/// Push [`WebviewSize`] to CEF in `PostUpdate` right after layout. The stock `resize` system runs in
/// `Update` on `Changed<WebviewSize>`, so it can lag one frame behind [`apply_pane_layout`] and leave a
/// browser painting into the wrong backing size (gray bands on stacked panes).
pub fn sync_cef_sizes_after_pane_layout(
    browsers: NonSend<Browsers>,
    mut last: Local<HashMap<Entity, (Vec2, f32)>>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    host_window: Query<&HostWindow>,
    panes: Query<(Entity, &WebviewSize), With<Pane>>,
) {
    let Ok(primary_e) = primary_window.single() else {
        return;
    };
    let default_scale = windows
        .get(primary_e)
        .ok()
        .map(|w| w.resolution.scale_factor())
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);

    let alive: HashSet<Entity> = panes.iter().map(|(e, _)| e).collect();
    last.retain(|e, _| alive.contains(e));

    for (entity, size) in &panes {
        if !browsers.has_browser(entity) {
            continue;
        }
        let scale = host_window
            .get(entity)
            .ok()
            .and_then(|h| windows.get(h.0).ok())
            .map(|w| w.resolution.scale_factor())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(default_scale);

        let sz = size.0;
        let unchanged = last
            .get(&entity)
            .is_some_and(|(pz, ps)| *pz == sz && (*ps - scale).abs() < 1.0e-4);
        if unchanged {
            continue;
        }
        last.insert(entity, (sz, scale));
        browsers.resize(&entity, sz, scale);
    }
}
