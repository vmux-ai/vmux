#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> pane_inner: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> pane_outer: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> border_color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> glow_params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> gradient_params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<uniform> border_accent: vec4<f32>;

fn sd_round_box_corners(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var rs = select(r.zw, r.xy, p.x > 0.0);
    let rad = select(rs.y, rs.x, p.y > 0.0);
    let q = abs(p) - b + vec2(rad);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - rad;
}

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

fn rounded_rect_sdf_alpha_aa(
    uv: vec2<f32>,
    uv_scale_w: f32,
    uv_scale_h: f32,
    box_w: f32,
    box_h: f32,
    r_px: f32,
    bottom_only: bool,
    aa_scale: f32,
) -> f32 {
    if r_px <= 0.0 {
        let p = vec2((uv.x - 0.5) * uv_scale_w, (uv.y - 0.5) * uv_scale_h);
        let b = vec2(box_w * 0.5, box_h * 0.5);
        let d = sd_round_box_corners(p, b, vec4(0.0));
        let aa = max(fwidth(d) * 1.5 * aa_scale, 1e-3);
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
    let aa = max(fwidth(d) * 1.5 * aa_scale, 1e-3);
    return 1.0 - smoothstep(-aa, aa, d);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let inner = pane_inner;
    let outer = pane_outer;
    let w_i = max(inner.y, 1.0);
    let h_i = max(inner.z, 1.0);
    let r_i = inner.x;
    let bottom_only = inner.w > 0.5;
    let w_o = max(outer.y, 1.0);
    let h_o = max(outer.z, 1.0);
    let r_o = outer.x;

    let a_outer = rounded_rect_sdf_alpha(uv, w_o, h_o, w_o, h_o, r_o, bottom_only);
    let a_inner = rounded_rect_sdf_alpha(uv, w_o, h_o, w_i, h_i, r_i, bottom_only);
    var ring = clamp(a_outer * (1.0 - a_inner), 0.0, 1.0);
    let ring_mix = pow(ring, 1.28);

    var stroke_rgb = border_color.rgb;
    if (gradient_params.x > 0.5) {
        let aspect = w_o / max(h_o, 1.0);
        let ang = atan2((uv.y - 0.5) * aspect, uv.x - 0.5);
        let wave = 0.5 + 0.5 * sin(ang * gradient_params.z + gradient_params.w * gradient_params.y);
        stroke_rgb = mix(border_color.rgb, border_accent.rgb, wave);
    }

    var rgb = stroke_rgb * ring_mix;
    var a = border_color.a * ring_mix;

    if (glow_params.x > 0.5 && glow_params.y > 1e-4) {
        let spread = max(glow_params.z, 0.5);
        let halo_o = rounded_rect_sdf_alpha_aa(uv, w_o, h_o, w_o, h_o, r_o, bottom_only, spread);
        let halo_i = rounded_rect_sdf_alpha_aa(uv, w_o, h_o, w_i, h_i, r_i, bottom_only, spread);
        var halo = clamp(halo_o * (1.0 - halo_i), 0.0, 1.0);
        let halo_strength = pow(halo, 1.15) * glow_params.y;
        rgb = rgb + stroke_rgb * halo_strength;
        a = clamp(a + halo_strength * border_color.a, 0.0, 1.0);
    }

    return vec4(rgb, a);
}
