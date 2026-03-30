//! World-space pane rectangles, [`WebviewSize`], depth bias, and CEF resize sync.

use bevy::ecs::system::SystemParam;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::Browsers;

use vmux_core::pane_corner_clip::{PANE_CORNER_CLIP_FULL, PANE_CORNER_CLIP_STATUS_BAR_BOTTOM};
use vmux_settings::VmuxAppSettings;
use vmux_ui::design::color;

use crate::loading_bar::{
    LoadingBarMaterial, LOADING_BAR_ANIM_TIME_SCALE, LOADING_BAR_DEPTH_BIAS_ABOVE_PANE,
    LOADING_BAR_HEIGHT_PX, PaneChromeLoadingBar,
    PendingNavigationLoads, webview_surface_is_placeholder,
};
use crate::{
    Active, CAMERA_DISTANCE, DEFAULT_PANE_CHROME_HEIGHT_PX, LayoutTree, Pane, PaneChromeOwner,
    PaneChromeStrip, PixelRect, Root, VmuxWorldCamera, solve_layout,
};

/// Bundles [`Res`] params for [`apply_pane_loading_bar_layout`] so the system stays within Bevy’s
/// system-parameter limit.
#[derive(SystemParam)]
pub struct LoadingBarTextureState<'w> {
    pub pending_nav: Res<'w, PendingNavigationLoads>,
    pub images: Res<'w, Assets<Image>>,
    pub webview_materials: Res<'w, Assets<WebviewExtendStandardMaterial>>,
}

/// Legacy constant (panes are laid out **coplanar** at `z = 0`; ordering uses [`PANE_DEPTH_BIAS_STRIDE`]).
pub const PANE_Z_STRIDE: f32 = 0.05;

/// Per-pane [`StandardMaterial::depth_bias`] step so stacked panes win the depth test (see [`apply_pane_layout`]).
pub const PANE_DEPTH_BIAS_STRIDE: f32 = 250.0;

/// Layout px inset per side between inner content rect and outer mesh; ring thickness in the shader.
pub const CHROME_BORDER_OUTSET_PX: f32 = 3.0;

fn sort_pane_rects_for_render_order(rects: &mut Vec<(Entity, PixelRect)>) {
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
}

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
/// Root [`PixelRect`] for [`solve_layout`], inset from the window by per-edge padding (clamped).
fn layout_root_area(vw: f32, vh: f32, left: f32, top: f32, right: f32, bottom: f32) -> PixelRect {
    if !vw.is_finite() || !vh.is_finite() || vw <= 0.0 || vh <= 0.0 {
        return PixelRect {
            x: 0.0,
            y: 0.0,
            w: vw.max(0.0),
            h: vh.max(0.0),
        };
    }
    // Leave at least MIN_SPAN total inner width/height for the grid.
    const MIN_SPAN: f32 = 2.0 * crate::MIN_PANE_PX;
    let max_lr = (vw - MIN_SPAN).max(0.0);
    let max_tb = (vh - MIN_SPAN).max(0.0);

    let mut l = left.max(0.0).min(max_lr);
    let mut r = right.max(0.0).min(max_lr);
    let mut t = top.max(0.0).min(max_tb);
    let mut b = bottom.max(0.0).min(max_tb);

    let sum_lr = l + r;
    if sum_lr > max_lr && sum_lr > 0.0 {
        let s = max_lr / sum_lr;
        l *= s;
        r *= s;
    }
    let sum_tb = t + b;
    if sum_tb > max_tb && sum_tb > 0.0 {
        let s = max_tb / sum_tb;
        t *= s;
        b *= s;
    }

    let w = (vw - l - r).max(0.0);
    let h = (vh - t - b).max(0.0);
    PixelRect { x: l, y: t, w, h }
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

/// Layout viewport in window pixels (same basis as [`apply_pane_layout`]) for chord handlers that need
/// pane rects without running the full layout system.
pub fn layout_viewport_for_workspace(window: &Window, camera: &Camera) -> Option<(f32, f32)> {
    let (vw, vh) = layout_viewport_px(window, camera);
    if vw.is_finite() && vh.is_finite() && vw > 0.0 && vh > 0.0 {
        Some((vw, vh))
    } else {
        None
    }
}

/// Pane rectangles for the current [`LayoutTree`] in workspace pixels (matches [`apply_pane_layout`]).
pub fn layout_workspace_pane_rects(
    vw: f32,
    vh: f32,
    layout: &LayoutTree,
    settings: &VmuxAppSettings,
    entity_alive: impl Fn(Entity) -> bool,
) -> Vec<(Entity, PixelRect)> {
    let s = settings.window_padding_px;
    let top = settings.window_padding_top_px;
    let area = layout_root_area(vw, vh, s, top, s, s);
    let mut rects = solve_layout(
        &layout.root,
        area,
        entity_alive,
        settings.pane_border_spacing_px,
        layout.zoom_pane,
    );
    sort_pane_rects_for_render_order(&mut rects);
    rects
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
    active: Query<Entity, (With<Pane>, With<Active>)>,
    mut transforms: Query<&mut Transform, (With<Pane>, Without<PaneChromeStrip>)>,
    mut sizes: Query<&mut WebviewSize, (With<Pane>, Without<PaneChromeStrip>)>,
    mut pane_vis: Query<&mut Visibility, (With<Pane>, Without<PaneChromeStrip>)>,
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

    let (vw, vh) = layout_viewport_px(window, camera);
    if !(vw.is_finite() && vh.is_finite()) || vw <= 0.0 || vh <= 0.0 {
        return;
    }

    let aspect = vw / vh;

    let tan_half_fov = (perspective.fov * 0.5).tan();
    let half_h = CAMERA_DISTANCE * tan_half_fov;
    let half_w = half_h * aspect;

    let entity_alive = |e: Entity| panes.contains(e);
    let s = settings.window_padding_px;
    let top = settings.window_padding_top_px;
    let area = layout_root_area(vw, vh, s, top, s, s);
    let mut rects = solve_layout(
        &layout.root,
        area,
        entity_alive,
        settings.pane_border_spacing_px,
        layout.zoom_pane,
    );
    sort_pane_rects_for_render_order(&mut rects);
    let rect_map: HashMap<Entity, PixelRect> = rects.iter().map(|(e, r)| (*e, *r)).collect();
    let pane_count = rects.len();

    let mut leaves = Vec::new();
    layout.root.collect_leaves(&mut leaves);

    let active_pane = active.iter().next().or_else(|| panes.iter().next());
    let o = CHROME_BORDER_OUTSET_PX;

    for (i, (entity, pr_full)) in rects.into_iter().enumerate() {
        let Ok(mut tf) = transforms.get_mut(entity) else {
            continue;
        };
        let Ok(mut ws) = sizes.get_mut(entity) else {
            continue;
        };
        if let Ok(mut v) = pane_vis.get_mut(entity) {
            *v = Visibility::Visible;
        }

        let is_active = active_pane == Some(entity);
        // Active pane: expand the mesh so the gutter exists in UV space; CEF backing stays inner size.
        let pr_mesh = if is_active {
            PixelRect {
                x: pr_full.x - o,
                y: pr_full.y - o,
                w: pr_full.w + 2.0 * o,
                h: pr_full.h + 2.0 * o,
            }
        } else {
            pr_full
        };

        let cx = pr_mesh.x + pr_mesh.w * 0.5;
        let cy = pr_mesh.y + pr_mesh.h * 0.5;
        let nx = cx / vw;
        let ny = cy / vh;

        let wx = (nx - 0.5) * 2.0 * half_w;
        let wy = (0.5 - ny) * 2.0 * half_h;

        let scale_x = (pr_mesh.w / vw) * half_w;
        let scale_y = (pr_mesh.h / vh) * half_h;

        // All panes share z = 0 so perspective projects the same logical pixel size for every tile
        // (including status strips). Stacking order uses depth_bias on materials, not translation.z.
        tf.translation = Vec3::new(wx, wy, 0.0);
        tf.scale = Vec3::new(scale_x.max(1.0e-4), scale_y.max(1.0e-4), 1.0);
        ws.0 = clamp_webview_backing_size(Vec2::new(pr_full.w.max(1.0), pr_full.h.max(1.0)));

        if let Ok(handle) = mesh_mat.get(entity)
            && let Some(mat) = materials.get_mut(handle.id())
        {
            mat.extension.pane_corner_clip = pane_corner_clip_uniform(
                settings.pane_border_radius_px,
                pr_full.w,
                pr_full.h,
                PANE_CORNER_CLIP_FULL,
            );
            if is_active {
                mat.extension.vmux_border_params = Vec4::new(1.0, o, pr_mesh.w, pr_mesh.h);
                mat.extension.vmux_border_color = color::active_pane_border_vec4();
            } else {
                mat.extension.vmux_border_params = Vec4::ZERO;
                mat.extension.vmux_border_color = Vec4::ZERO;
            }
            // Active pane mesh is expanded by `o` into neighbors’ tiles; without a bias boost, coplanar
            // panes with a higher render index would win the depth test and hide the ring.
            let base_bias = i as f32 * PANE_DEPTH_BIAS_STRIDE;
            mat.base.depth_bias = if is_active {
                base_bias + pane_count as f32 * PANE_DEPTH_BIAS_STRIDE
            } else {
                base_bias
            };
        }
    }

    // Panes not in the layout result (e.g. tmux zoom hides other leaves): collapse and hide so
    // transforms don’t stick from the previous frame.
    for leaf in leaves {
        if rect_map.contains_key(&leaf) {
            continue;
        }
        let Ok(mut tf) = transforms.get_mut(leaf) else {
            continue;
        };
        let Ok(mut ws) = sizes.get_mut(leaf) else {
            continue;
        };
        if let Ok(mut v) = pane_vis.get_mut(leaf) {
            *v = Visibility::Hidden;
        }
        tf.translation = Vec3::ZERO;
        tf.scale = Vec3::splat(1.0e-4);
        ws.0 = Vec2::splat(1.0);
        if let Ok(handle) = mesh_mat.get(leaf)
            && let Some(mat) = materials.get_mut(handle.id())
        {
            mat.extension.vmux_border_params = Vec4::ZERO;
            mat.extension.vmux_border_color = Vec4::ZERO;
            mat.base.depth_bias = -1_000_000.0;
        }
    }
}

/// Shared viewport + `solve_layout` rect map for [`apply_pane_chrome_layout`] (Bevy caps per-system parameters).
struct ChromeLayoutFrame {
    vw: f32,
    vh: f32,
    half_w: f32,
    half_h: f32,
    rect_map: HashMap<Entity, PixelRect>,
    /// Same pane enumeration as [`apply_pane_layout`] (for depth_bias on chrome / loading bar).
    pane_index: HashMap<Entity, usize>,
    active_pane: Option<Entity>,
}

fn chrome_layout_frame(
    window: &Window,
    camera: &Camera,
    layout: &LayoutTree,
    settings: &VmuxAppSettings,
    panes: &Query<Entity, With<Pane>>,
    active: &Query<Entity, (With<Pane>, With<Active>)>,
    perspective_fov_y: f32,
) -> Option<ChromeLayoutFrame> {
    let (vw, vh) = layout_viewport_px(window, camera);
    if !(vw.is_finite() && vh.is_finite()) || vw <= 0.0 || vh <= 0.0 {
        return None;
    }
    let aspect = vw / vh;
    let tan_half_fov = (perspective_fov_y * 0.5).tan();
    let half_h = CAMERA_DISTANCE * tan_half_fov;
    let half_w = half_h * aspect;
    let entity_alive = |e: Entity| panes.contains(e);
    let s = settings.window_padding_px;
    let top = settings.window_padding_top_px;
    let area = layout_root_area(vw, vh, s, top, s, s);
    let mut rects: Vec<(Entity, PixelRect)> = solve_layout(
        &layout.root,
        area,
        entity_alive,
        settings.pane_border_spacing_px,
        layout.zoom_pane,
    )
    .into_iter()
    .collect();
    sort_pane_rects_for_render_order(&mut rects);
    let pane_index: HashMap<Entity, usize> = rects
        .iter()
        .enumerate()
        .map(|(i, (e, _))| (*e, i))
        .collect();
    let rect_map: HashMap<Entity, PixelRect> = rects.iter().map(|(e, r)| (*e, *r)).collect();
    let active_pane = active.iter().next().or_else(|| panes.iter().next());
    Some(ChromeLayoutFrame {
        vw,
        vh,
        half_w,
        half_h,
        rect_map,
        pane_index,
        active_pane,
    })
}

/// Position per-pane chrome strips as a bottom **overlay** on each pane tile (after [`apply_pane_layout`]).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
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
    let Some(frame) = chrome_layout_frame(
        window,
        camera,
        layout,
        &settings,
        &panes,
        &active,
        perspective.fov,
    ) else {
        return;
    };
    let ChromeLayoutFrame {
        vw,
        vh,
        half_w,
        half_h,
        rect_map,
        active_pane,
        ..
    } = frame;

    for (i, (chrome_ent, owner)) in chrome_q.iter().enumerate() {
        let Some(pr_full) = rect_map.get(&owner.0).copied() else {
            if let Ok(mut v) = vis.get_mut(chrome_ent) {
                *v = Visibility::Hidden;
            }
            continue;
        };
        let (_, mut chrome_pr) =
            split_pane_content_and_chrome(pr_full, DEFAULT_PANE_CHROME_HEIGHT_PX);
        if chrome_pr.h <= 0.0 {
            if let Ok(mut v) = vis.get_mut(chrome_ent) {
                *v = Visibility::Hidden;
            }
            continue;
        }
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

        let (mut trans, scale, layout_px) =
            pixel_rect_to_world_plane(chrome_pr, vw, vh, half_w, half_h);
        let z_base = pane_tf.get(owner.0).map(|t| t.translation.z).unwrap_or(0.0);
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

/// Indeterminate bar along the bottom of the main content rect (above the status strip), or the tile bottom if there is no strip.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn apply_pane_loading_bar_layout(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &Projection), (With<Camera3d>, With<VmuxWorldCamera>)>,
    layout_q: Query<&LayoutTree, With<Root>>,
    settings: Res<VmuxAppSettings>,
    time: Res<Time>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    panes: Query<Entity, With<Pane>>,
    pane_tf: Query<&Transform, (With<Pane>, Without<PaneChromeStrip>)>,
    loading_q: Query<(Entity, &PaneChromeOwner), With<PaneChromeLoadingBar>>,
    mut loading_tf: Query<&mut Transform, (With<PaneChromeLoadingBar>, Without<Pane>)>,
    loading_mesh_mat: Query<
        &MeshMaterial3d<LoadingBarMaterial>,
        (With<PaneChromeLoadingBar>, Without<Pane>),
    >,
    mut loading_materials: ResMut<Assets<LoadingBarMaterial>>,
    mut loading_vis: Query<&mut Visibility, (With<PaneChromeLoadingBar>, Without<Pane>)>,
    pane_mesh_mat: Query<
        &MeshMaterial3d<WebviewExtendStandardMaterial>,
        (With<Pane>, Without<PaneChromeStrip>),
    >,
    tex: LoadingBarTextureState,
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
    let Some(frame) = chrome_layout_frame(
        window,
        camera,
        layout,
        &settings,
        &panes,
        &active,
        perspective.fov,
    ) else {
        return;
    };
    let ChromeLayoutFrame {
        vw,
        vh,
        half_w,
        half_h,
        rect_map,
        pane_index,
        ..
    } = frame;

    for (lb_ent, owner) in loading_q.iter() {
        let Some(pr_full) = rect_map.get(&owner.0).copied() else {
            if let Ok(mut v) = loading_vis.get_mut(lb_ent) {
                *v = Visibility::Hidden;
            }
            continue;
        };
        // Bottom of **main content** only — same split + horizontal clamp as chrome, so the bar sits
        // flush above the status strip without overlapping [`PaneChromeStrip`].
        let (mut content_pr, _) =
            split_pane_content_and_chrome(pr_full, DEFAULT_PANE_CHROME_HEIGHT_PX);
        let r_edge = pr_full.x + pr_full.w;
        content_pr.x = content_pr.x.clamp(pr_full.x, r_edge);
        content_pr.w = content_pr.w.min(r_edge - content_pr.x).max(0.0);

        let bar_pr = if content_pr.h > 0.0 && content_pr.w > 0.0 {
            let h = LOADING_BAR_HEIGHT_PX.min(content_pr.h.max(0.0));
            if h <= 0.0 {
                if let Ok(mut v) = loading_vis.get_mut(lb_ent) {
                    *v = Visibility::Hidden;
                }
                continue;
            }
            PixelRect {
                x: content_pr.x,
                y: content_pr.y + content_pr.h - h,
                w: content_pr.w,
                h,
            }
        } else {
            let h = LOADING_BAR_HEIGHT_PX.min(pr_full.h.max(0.0));
            if pr_full.w <= 0.0 || h <= 0.0 {
                if let Ok(mut v) = loading_vis.get_mut(lb_ent) {
                    *v = Visibility::Hidden;
                }
                continue;
            }
            PixelRect {
                x: pr_full.x,
                y: pr_full.y + pr_full.h - h,
                w: pr_full.w,
                h,
            }
        };

        let placeholder = pane_mesh_mat
            .get(owner.0)
            .ok()
            .and_then(|h| tex.webview_materials.get(h.id()))
            .is_some_and(|m| webview_surface_is_placeholder(&tex.images, m));
        let loading = placeholder || tex.pending_nav.0.contains_key(&owner.0);

        if let Ok(mut v) = loading_vis.get_mut(lb_ent) {
            *v = if loading {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if !loading {
            continue;
        }

        let z_base = pane_tf.get(owner.0).map(|t| t.translation.z).unwrap_or(0.0);
        let (b_trans, b_scale, _) = pixel_rect_to_world_plane(bar_pr, vw, vh, half_w, half_h);
        let mut bt = b_trans;
        bt.z = z_base;
        if let Ok(mut tf) = loading_tf.get_mut(lb_ent) {
            tf.translation = bt;
            tf.scale = b_scale;
        }
        let i = pane_index.get(&owner.0).copied().unwrap_or(0);
        if let Ok(bhandle) = loading_mesh_mat.get(lb_ent)
            && let Some(bmat) = loading_materials.get_mut(bhandle.id())
        {
            bmat.anim = Vec4::new(
                time.elapsed_secs() * LOADING_BAR_ANIM_TIME_SCALE,
                bar_pr.w.max(1.0),
                bar_pr.h.max(1.0),
                0.0,
            );
            bmat.depth_bias =
                i as f32 * PANE_DEPTH_BIAS_STRIDE + LOADING_BAR_DEPTH_BIAS_ABOVE_PANE;
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
