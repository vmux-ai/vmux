#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> anim: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> track_rgba: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> sweep_rgba: vec4<f32>;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let t = anim.x;
    let u = mesh.uv.x;
    let v = mesh.uv.y;
    let pi2 = 6.28318530718;

    // Dominant traveling lobes (high d(phase)/dt so motion reads clearly on screen).
    let scroll = u * pi2 * 2.2 - t * 7.8;
    let wave_a = sin(scroll) * 0.5 + 0.5;
    let wave_b = sin(u * pi2 * 1.1 - t * 4.2 + 0.9) * 0.5 + 0.5;
    // Interference + fine shimmer (fast ripples across time and space).
    let beat = sin(scroll * 0.5 + wave_b * pi2) * 0.5 + 0.5;
    let shimmer = sin(u * pi2 * 14.0 + t * 18.0 + v * pi2 * 3.0) * 0.5 + 0.5;

    var flow = wave_a * 0.38 + wave_b * 0.28 + beat * 0.22 + shimmer * 0.12;
    flow = flow * flow;
    // Stretch contrast: dim stays dim, bright moments read as motion.
    flow = pow(flow, 0.42);
    flow = clamp(flow, 0.0, 1.0);

    let sweep_w = mix(0.04, 1.0, flow);
    let tr = track_rgba;
    let sw = sweep_rgba;
    var rgb = mix(tr.rgb * tr.a, sw.rgb, sweep_w);
    rgb = min(rgb * vec3(1.32, 1.24, 1.18), vec3(1.0));
    let pulse_a = mix(0.2, 1.0, flow);
    let a = mix(tr.a * 0.75, sw.a, pulse_a);
    return vec4(rgb, min(a * 1.08, 1.0));
}
