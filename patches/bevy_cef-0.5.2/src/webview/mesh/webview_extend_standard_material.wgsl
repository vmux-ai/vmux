#define_import_path webview::standard_material;

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import webview::util::{
    surface_color,
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let s = surface_color(in.uv);
    let rgb = s.rgb * (pbr_input.material.base_color.rgb + pbr_input.material.emissive.rgb);
    let c = vec4(rgb, s.a * pbr_input.material.base_color.a);
    var out: FragmentOutput;
    out.color = alpha_discard(pbr_input.material, c);
    return out;
}
