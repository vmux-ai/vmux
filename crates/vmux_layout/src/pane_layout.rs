//! World-space pane rectangles, [`WebviewSize`], depth bias, and CEF resize sync.

use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::Browsers;

use vmux_core::pane_corner_clip::{
    PANE_CORNER_CLIP_FULL, PANE_CORNER_CLIP_STATUS_BAR_BOTTOM,
};
use vmux_settings::VmuxAppSettings;

use crate::{
    Active, CAMERA_DISTANCE, DEFAULT_PANE_CHROME_HEIGHT_PX, LayoutTree, Pane, PaneChromeOwner,
    PaneChromeStrip, PixelRect, Root, VmuxWorldCamera, solve_layout,
};

/// Legacy constant (panes are laid out **coplanar** at `z = 0`; ordering uses [`PANE_DEPTH_BIAS_STRIDE`]).
pub const PANE_Z_STRIDE: f32 = 0.05;

/// Per-pane [`StandardMaterial::depth_bias`] step so stacked panes win the depth test (see [`apply_pane_layout`]).
const PANE_DEPTH_BIAS_STRIDE: f32 = 250.0;

/// Upper bound on CEF OSR backing size (longest side in layout pixels). Uncapped sizes track the
/// full window/pane pixel area and are very expensive for typing/compositing; the mesh still fills
/// the pane — the texture is upscaled slightly when capped.
const MAX_CEF_BACKING_LONG_SIDE: f32 = 1536.0;

/// Pixel size for pane ↔ world mapping.
///
/// Prefer [`Window`] width/height when valid so layout tracks resize immediately. On some frames
/// during window resize, `Camera::logical_viewport_size()` can lag behind [`Window`]; using the
/// smaller/stale viewport for `solve_layout` while normalizing with a different effective size
/// widens pane and chrome strips (notably the active pane’s status bar spilling past the split).
/// Root [`PixelRect`] for [`solve_layout`], inset from the window by `edge_gap_px` (clamped).
fn layout_root_area(vw: f32, vh: f32, edge_gap_px: f32) -> PixelRect {
    if !edge_gap_px.is_finite() || edge_gap_px <= 0.0 || !vw.is_finite() || !vh.is_finite() {
        return PixelRect {
            x: 0.0,
            y: 0.0,
            w: vw,
            h: vh,
        };
    }
    // Leave at least one minimal pane worth of span in each axis for the inner region.
    const MIN_SPAN: f32 = 2.0 * crate::MIN_PANE_PX;
    let max_g_w = ((vw - MIN_SPAN) * 0.5).max(0.0);
    let max_g_h = ((vh - MIN_SPAN) * 0.5).max(0.0);
    let g = edge_gap_px.max(0.0).min(max_g_w).min(max_g_h);
    let w = (vw - 2.0 * g).max(0.0);
    let h = (vh - 2.0 * g).max(0.0);
    PixelRect {
        x: g,
        y: g,
        w,
        h,
    }
}

fn layout_viewport_px(window: &Window, camera: &Camera) -> (f32, f32) {
    let vw = window.width();
    let vh = window.height();
    if vw.is_finite() && vh.is_finite() && vw > 0.0 && vh > 0.0 {
        return (vw, vh);
    }
    if let Some(size) = camera.logical_viewport_size()
        && size.x > 0.0
        && size.y > 0.0
        && size.x.is_finite()
        && size.y.is_finite()
    {
        return (size.x, size.y);
    }
    (vw, vh)
}

/// `x` = corner radius (layout px), `y`/`z` = tile size (layout px), `w` = clip mode ([`vmux_core::pane_corner_clip`]).
fn pane_corner_clip_uniform(px: f32, rect_w: f32, rect_h: f32, clip_mode: f32) -> Vec4 {
    if !px.is_finite() || px <= 0.0 {
        return Vec4::ZERO;
    }
    let w = rect_w.max(1.0e-6);
    let h = rect_h.max(1.0e-6);
    let m = w.min(h);
    let r_px = px.min(m * 0.5).max(0.0);
    Vec4::new(r_px, w, h, clip_mode)
}

/// Same backing-size cap as pane webviews (see [`apply_pane_layout`]).
#[inline]
pub fn clamp_webview_backing_size(layout_px: Vec2) -> Vec2 {
    let w = layout_px.x.max(1.0);
    let h = layout_px.y.max(1.0);
    let m = w.max(h);
    if m <= MAX_CEF_BACKING_LONG_SIDE {
        return Vec2::new(w, h);
    }
    let s = MAX_CEF_BACKING_LONG_SIDE / m;
    Vec2::new((w * s).max(1.0), (h * s).max(1.0))
}

/// Map a [`PixelRect`] in window pixels to world XY plane space (same convention as [`apply_pane_layout`]).
/// Translation `z` is `0.0`; set `translation.z` for stacking in front of panes.
pub fn pixel_rect_to_world_plane(
    pr: PixelRect,
    vw: f32,
    vh: f32,
    half_w: f32,
    half_h: f32,
) -> (Vec3, Vec3, Vec2) {
    let cx = pr.x + pr.w * 0.5;
    let cy = pr.y + pr.h * 0.5;
    let nx = cx / vw;
    let ny = cy / vh;
    let wx = (nx - 0.5) * 2.0 * half_w;
    let wy = (0.5 - ny) * 2.0 * half_h;
    let scale_x = (pr.w / vw) * half_w;
    let scale_y = (pr.h / vh) * half_h;
    let translation = Vec3::new(wx, wy, 0.0);
    let scale = Vec3::new(scale_x.max(1.0e-4), scale_y.max(1.0e-4), 1.0);
    let layout_px = Vec2::new(pr.w.max(1.0), pr.h.max(1.0));
    (translation, scale, layout_px)
}

/// Minimum main content height when splitting off chrome (matches leaf solver).
const MIN_PANE_CONTENT_PX: f32 = 48.0;

/// Split a pane tile into **content** (top) and **chrome** (bottom) pixel rects.
///
/// Main [`Pane`] webviews use the **full** tile ([`apply_pane_layout`]); this split is for the
/// [`PaneChromeStrip`] overlay so it sits **on top** of the page at the bottom (same geometry as
/// `chrome`).
pub fn split_pane_content_and_chrome(
    full: PixelRect,
    desired_chrome_px: f32,
) -> (PixelRect, PixelRect) {
    let full_h = full.h.max(1.0);
    let mut chrome_h = desired_chrome_px.min(full_h * 0.5).max(0.0);
    chrome_h = chrome_h.min((full_h - MIN_PANE_CONTENT_PX).max(0.0));
    if chrome_h > 0.0 && chrome_h < 8.0 {
        chrome_h = if full_h >= MIN_PANE_CONTENT_PX + 8.0 {
            8.0
        } else {
            0.0
        };
    }
    // Integer px height so every pane’s strip matches in backing size and on screen.
    chrome_h = chrome_h.round().max(0.0);
    let content_h = (full_h - chrome_h).max(1.0);
    let content = PixelRect {
        x: full.x,
        y: full.y,
        w: full.w,
        h: content_h,
    };
    let chrome = PixelRect {
        x: full.x,
        y: full.y + content_h,
        w: full.w,
        h: chrome_h.max(0.0),
    };
    (content, chrome)
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn apply_pane_layout(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &Projection), (With<Camera3d>, With<VmuxWorldCamera>)>,
    layout_q: Query<&LayoutTree, With<Root>>,
    settings: Res<VmuxAppSettings>,
    panes: Query<Entity, With<Pane>>,
    mut transforms: Query<&mut Transform, (With<Pane>, Without<PaneChromeStrip>)>,
    mut sizes: Query<&mut WebviewSize, (With<Pane>, Without<PaneChromeStrip>)>,
    mesh_mat: Query<
        &MeshMaterial3d<WebviewExtendStandardMaterial>,
        (With<Pane>, Without<PaneChromeStrip>),
    >,
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

    let (vw, vh) = layout_viewport_px(&window, camera);
    if !(vw.is_finite() && vh.is_finite()) || vw <= 0.0 || vh <= 0.0 {
        return;
    }

    let aspect = vw / vh;

    let tan_half_fov = (perspective.fov * 0.5).tan();
    let half_h = CAMERA_DISTANCE * tan_half_fov;
    let half_w = half_h * aspect;

    let entity_alive = |e: Entity| panes.contains(e);
    let area = layout_root_area(vw, vh, settings.window_padding_px);
    let mut rects = solve_layout(&layout.root, area, entity_alive, settings.pane_border_spacing_px);
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

    for (i, (entity, pr_full)) in rects.into_iter().enumerate() {
        let Ok(mut tf) = transforms.get_mut(entity) else {
            continue;
        };
        let Ok(mut ws) = sizes.get_mut(entity) else {
            continue;
        };

        // Full tile for the main webview; status chrome is a separate mesh in front (see
        // [`apply_pane_chrome_layout`]) so inactive panes don’t reserve an empty strip band.
        let pr = pr_full;

        let cx = pr.x + pr.w * 0.5;
        let cy = pr.y + pr.h * 0.5;
        let nx = cx / vw;
        let ny = cy / vh;

        let wx = (nx - 0.5) * 2.0 * half_w;
        let wy = (0.5 - ny) * 2.0 * half_h;

        let scale_x = (pr.w / vw) * half_w;
        let scale_y = (pr.h / vh) * half_h;

        // All panes share z = 0 so perspective projects the same logical pixel size for every tile
        // (including status strips). Stacking order uses depth_bias on materials, not translation.z.
        tf.translation = Vec3::new(wx, wy, 0.0);
        tf.scale = Vec3::new(scale_x.max(1.0e-4), scale_y.max(1.0e-4), 1.0);
        ws.0 = clamp_webview_backing_size(Vec2::new(pr.w.max(1.0), pr.h.max(1.0)));

        if let Ok(handle) = mesh_mat.get(entity)
            && let Some(mat) = materials.get_mut(handle.id())
        {
            mat.extension.pane_corner_clip = pane_corner_clip_uniform(
                settings.pane_border_radius_px,
                pr.w,
                pr.h,
                PANE_CORNER_CLIP_FULL,
            );
            mat.base.depth_bias = i as f32 * PANE_DEPTH_BIAS_STRIDE;
        }
    }
}

/// Position per-pane chrome strips as a bottom **overlay** on each pane tile (after [`apply_pane_layout`]).
#[allow(clippy::too_many_arguments)]
pub fn apply_pane_chrome_layout(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &Projection), (With<Camera3d>, With<VmuxWorldCamera>)>,
    layout_q: Query<&LayoutTree, With<Root>>,
    settings: Res<VmuxAppSettings>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    panes: Query<Entity, With<Pane>>,
    pane_tf: Query<&Transform, (With<Pane>, Without<PaneChromeStrip>)>,
    chrome_q: Query<(Entity, &PaneChromeOwner), With<PaneChromeStrip>>,
    mut transforms: Query<&mut Transform, (With<PaneChromeStrip>, Without<Pane>)>,
    mut sizes: Query<&mut WebviewSize, (With<PaneChromeStrip>, Without<Pane>)>,
    mesh_mat: Query<
        &MeshMaterial3d<WebviewExtendStandardMaterial>,
        (With<PaneChromeStrip>, Without<Pane>),
    >,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut vis: Query<&mut Visibility, (With<PaneChromeStrip>, Without<Pane>)>,
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

    let (vw, vh) = layout_viewport_px(&window, camera);
    if !(vw.is_finite() && vh.is_finite()) || vw <= 0.0 || vh <= 0.0 {
        return;
    }

    let aspect = vw / vh;

    let tan_half_fov = (perspective.fov * 0.5).tan();
    let half_h = CAMERA_DISTANCE * tan_half_fov;
    let half_w = half_h * aspect;

    let entity_alive = |e: Entity| panes.contains(e);
    let area = layout_root_area(vw, vh, settings.window_padding_px);
    let rects = solve_layout(&layout.root, area, entity_alive, settings.pane_border_spacing_px);
    let rect_map: HashMap<Entity, PixelRect> = rects.into_iter().collect();

    let active_pane = active.iter().next().or_else(|| panes.iter().next());
    for (i, (chrome_ent, owner)) in chrome_q.iter().enumerate() {
        let Some(pr_full) = rect_map.get(&owner.0).copied() else {
            continue;
        };
        let (_, mut chrome_pr) = split_pane_content_and_chrome(pr_full, DEFAULT_PANE_CHROME_HEIGHT_PX);
        if chrome_pr.h <= 0.0 {
            if let Ok(mut v) = vis.get_mut(chrome_ent) {
                *v = Visibility::Hidden;
            }
            continue;
        }
        // Hard clamp to owner tile so the strip never extends past the split (layout float / resize).
        let r = pr_full.x + pr_full.w;
        chrome_pr.x = chrome_pr.x.clamp(pr_full.x, r);
        chrome_pr.w = chrome_pr.w.min(r - chrome_pr.x).max(0.0);
        if chrome_pr.w <= 0.0 || chrome_pr.h <= 0.0 {
            if let Ok(mut v) = vis.get_mut(chrome_ent) {
                *v = Visibility::Hidden;
            }
            continue;
        }

        let is_active = active_pane == Some(owner.0);
        if let Ok(mut v) = vis.get_mut(chrome_ent) {
            *v = if is_active {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }

        // Inactive strips stay hidden but still get layout + WebviewSize so focus switches don’t jump.
        let (mut trans, scale, layout_px) =
            pixel_rect_to_world_plane(chrome_pr, vw, vh, half_w, half_h);
        let z_base = pane_tf.get(owner.0).map(|t| t.translation.z).unwrap_or(0.0);
        // Same Z as the owner pane so perspective does not change strip height between panes
        // (offsetting in Z made left/right splits look mismatched). Draw order uses depth_bias.
        trans.z = z_base;

        let Ok(mut tf) = transforms.get_mut(chrome_ent) else {
            continue;
        };
        tf.translation = trans;
        tf.scale = scale;

        let Ok(mut ws) = sizes.get_mut(chrome_ent) else {
            continue;
        };
        ws.0 = clamp_webview_backing_size(Vec2::new(
            layout_px.x.round().max(1.0),
            layout_px.y.round().max(1.0),
        ));

        if let Ok(handle) = mesh_mat.get(chrome_ent)
            && let Some(mat) = materials.get_mut(handle.id())
        {
            // Status strip: bottom-only rounding — contract in `vmux_status_bar::pane_corner_clip`.
            mat.extension.pane_corner_clip = pane_corner_clip_uniform(
                settings.pane_border_radius_px,
                chrome_pr.w,
                chrome_pr.h,
                PANE_CORNER_CLIP_STATUS_BAR_BOTTOM,
            );
            mat.base.depth_bias = 1_000_000.0 + i as f32;
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
    panes: Query<(Entity, &WebviewSize), Or<(With<Pane>, With<PaneChromeStrip>)>>,
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
