
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

/// Rounded rect in **layout pixel space**; `w` = 1 means only bottom corners rounded (status bar).
fn rounded_rect_cover(uv: vec2<f32>) -> f32 {
    let r_px = webview_corner.x;
    let w_px = max(webview_corner.y, 1.0);
    let h_px = max(webview_corner.z, 1.0);
    let bottom_only = webview_corner.w > 0.5;
    if (r_px <= 0.0) {
        return 1.0;
    }
    let r_cap = min(r_px, 0.5 * min(w_px, h_px));
    let p = vec2((uv.x - 0.5) * w_px, (uv.y - 0.5) * h_px);
    let b = vec2(w_px * 0.5, h_px * 0.5);
    let radii = select(
        vec4(r_cap, r_cap, r_cap, r_cap),
        vec4(r_cap, 0.0, r_cap, 0.0),
        bottom_only,
    );
    let d = sd_round_box_corners(p, b, radii);
    let aa = fwidth(d) * 1.5;
    return 1.0 - smoothstep(-aa, aa, d);
}

fn surface_color(uv: vec2<f32>) -> vec4<f32> {
    let c = textureSampleBias(surface_texture, surface_sampler, uv, view.mip_bias);
    let cover = rounded_rect_cover(uv);
    return vec4(c.rgb, c.a * cover);
}