#define_import_path vmux_desktop::display_panel_material

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    pbr_bindings::material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> panel_clip: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> panel_corner_mode: vec4<f32>;

// Per-corner round box SDF (IQ style):
// r = (bottom-right, top-right, bottom-left, top-left)
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
    let w = max(box_w, 1.0);
    let h = max(box_h, 1.0);
    let r_cap = min(max(r_px, 0.0), 0.5 * min(w, h));

    // Map UV [0,1] to centered pixel-like space.
    let p = vec2((uv.x - 0.5) * uv_scale_w, (uv.y - 0.5) * uv_scale_h);
    let b = vec2(w * 0.5, h * 0.5);

    let radii = select(
        vec4(r_cap, r_cap, r_cap, r_cap), // all corners
        vec4(r_cap, 0.0, r_cap, 0.0),     // bottom-only
        bottom_only
    );

    let d = sd_round_box_corners(p, b, radii);
    let aa = max(fwidth(d) * 1.5, 1e-3);
    return 1.0 - smoothstep(-aa, aa, d);
}

fn panel_cover(uv: vec2<f32>) -> f32 {
    let r_px = panel_clip.x;
    let w_m = max(panel_clip.y, 1e-6);
    let h_m = max(panel_clip.z, 1e-6);
    let ppm = max(panel_clip.w, 1.0);
    let w_px = w_m * ppm;
    let h_px = h_m * ppm;
    let bottom_only = panel_corner_mode.x > 0.5;
    return rounded_rect_sdf_alpha(uv, w_px, h_px, w_px, h_px, r_px, bottom_only);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let cover = panel_cover(in.uv);
    pbr_input.material.base_color *= vec4(1.0, 1.0, 1.0, cover);

    // Respect StandardMaterial alpha mode/discard behavior.
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    if (material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    } else {
        out.color = pbr_input.material.base_color;
    }
    return out;
}
