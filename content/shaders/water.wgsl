// Water surface (#67): the planet's surface colour, more opaque at grazing angles, and a
// gentle moving ripple in the normal (two layers of solid noise in planet space). No
// reflections, no simulation. Colours come from the recipe (uniform).
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    mesh_view_bindings::globals,
}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::{prepass_io::{VertexOutput, FragmentOutput}, pbr_deferred_functions::deferred_output}
#else
#import bevy_pbr::{forward_io::{VertexOutput, FragmentOutput}, pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing}}
#endif

struct WaterLook {
    // xyz planet centre in render space
    centre: vec4<f32>,
    // rgb surface colour, w opacity
    surface: vec4<f32>,
    // x ripple size (m), y speed (m/s)
    ripple: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: WaterLook;

fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * 0.1031);
    let r = q + dot(q, q.yzx + 33.33);
    return fract((r.x + r.y) * r.z);
}

fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash3(i), hash3(i + vec3(1.0, 0.0, 0.0)), u.x), mix(hash3(i + vec3(0.0, 1.0, 0.0)), hash3(i + vec3(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(hash3(i + vec3(0.0, 0.0, 1.0)), hash3(i + vec3(1.0, 0.0, 1.0)), u.x), mix(hash3(i + vec3(0.0, 1.0, 1.0)), hash3(i + vec3(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

fn waves(p: vec3<f32>, t: f32) -> f32 {
    return value_noise(p + vec3(t, 0.0, t * 0.7)) + 0.5 * value_noise(p * 2.3 - vec3(t * 0.8, t * 0.3, 0.0));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let rel = in.world_position.xyz - water.centre.xyz;
    var up = normalize(rel);
    if !is_front {
        up = -up;
    }
    let size = max(water.ripple.x, 0.1);
    let t = globals.time * water.ripple.y / size;
    let p = rel / size;
    var tx = normalize(cross(up, vec3(0.0, 1.0, 0.0001)));
    let ty = cross(up, tx);
    let e = 0.2;
    let h0 = waves(p, t);
    let dx = (waves(p + tx * e, t) - h0) / e;
    let dy = (waves(p + ty * e, t) - h0) / e;
    let n = normalize(up - (tx * dx + ty * dy) * 0.12);
    pbr_input.N = n;
    pbr_input.world_normal = n;
    let facing = clamp(dot(n, pbr_input.V), 0.0, 1.0);
    let alpha = mix(water.surface.a, 1.0, pow(1.0 - facing, 4.0));
    pbr_input.material.base_color = vec4(water.surface.rgb, alpha);
#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
