
#define_import_path webview::util

#import bevy_pbr::{
    mesh_view_bindings::view,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(101) var surface_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var surface_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var<uniform> webview_corner: vec4<f32>;

/// Per-corner round box (IQ), `r` = (bottom-right, top-right, bottom-left, top-left) in layout px.
fn sd_round_box_corners(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var rs = select(r.zw, r.xy, p.x > 0.0);
    let rad = select(rs.y, rs.x, p.y > 0.0);
    let q = abs(p) - b + vec2(rad);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - rad;
}

/// `uv` maps to a rect of size `uv_scale_w` × `uv_scale_h` layout px; SDF for inner box `box_w` × `box_h` centered.
fn rounded_rect_sdf_alpha(
    uv: vec2<f32>,
    uv_scale_w: f32,
    uv_scale_h: f32,
    box_w: f32,
    box_h: f32,
    r_px: f32,
    bottom_only: bool,
) -> f32 {
    if r_px <= 0.0 {
        let p = vec2((uv.x - 0.5) * uv_scale_w, (uv.y - 0.5) * uv_scale_h);
        let b = vec2(box_w * 0.5, box_h * 0.5);
        let d = sd_round_box_corners(p, b, vec4(0.0));
        let aa = max(fwidth(d) * 1.5, 1e-3);
        return 1.0 - smoothstep(-aa, aa, d);
    }
    let w = max(box_w, 1.0);
    let h = max(box_h, 1.0);
    let r_cap = min(r_px, 0.5 * min(w, h));
    let p = vec2((uv.x - 0.5) * uv_scale_w, (uv.y - 0.5) * uv_scale_h);
    let b = vec2(w * 0.5, h * 0.5);
    let radii = select(
        vec4(r_cap, r_cap, r_cap, r_cap),
        vec4(r_cap, 0.0, r_cap, 0.0),
        bottom_only,
    );
    let d = sd_round_box_corners(p, b, radii);
    let aa = max(fwidth(d) * 1.5, 1e-3);
    return 1.0 - smoothstep(-aa, aa, d);
}

/// Rounded rect in **layout pixel space**; `w` = 1 means only bottom corners rounded (status bar).
fn rounded_rect_cover(uv: vec2<f32>) -> f32 {
    let r_px = webview_corner.x;
    let w_px = max(webview_corner.y, 1.0);
    let h_px = max(webview_corner.z, 1.0);
    let bottom_only = webview_corner.w > 0.5;
    return rounded_rect_sdf_alpha(uv, w_px, h_px, w_px, h_px, r_px, bottom_only);
}

fn surface_color(uv: vec2<f32>) -> vec4<f32> {
    let c = textureSampleBias(surface_texture, surface_sampler, uv, view.mip_bias);
    let cover = rounded_rect_cover(uv);
    return vec4(c.rgb, c.a * cover);
}
