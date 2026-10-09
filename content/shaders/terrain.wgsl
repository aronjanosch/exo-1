// Terrain material (#66): StandardMaterial lighting with our own base colour.
// Ground on flats, rock on steep faces (radial up per fragment), strata bands on rock keyed by
// height above the base radius, a cap high up, a grain near the camera. Patterns are solid 3D
// noise in planet space: no projection, so no seams at chunk or cube-face edges and no
// stretching on cliffs (what triplanar mapping is for with textures). No colour constants here:
// every colour comes from the recipe (vertex palette and the uniform).
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::view,
}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::{prepass_io::{VertexOutput, FragmentOutput}, pbr_deferred_functions::deferred_output}
#else
#import bevy_pbr::{forward_io::{VertexOutput, FragmentOutput}, pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing}}
#endif

struct TerrainLook {
    // xyz planet centre in render space, w base radius
    centre: vec4<f32>,
    // x sea height above the base radius, y/z rock slope start/full (deg), w strata band (m)
    shape: vec4<f32>,
    strata0: vec4<f32>,
    strata1: vec4<f32>,
    strata2: vec4<f32>,
    strata3: vec4<f32>,
    // rgb cap colour, w cap height above sea (m)
    cap: vec4<f32>,
    // x cap fade (m), y strata jitter (m), z detail strength, w detail far (m)
    misc: vec4<f32>,
    // x number of strata colours
    counts: vec4<f32>,
    // rgb ground tint deep under water, w depth (m) where it is full
    water: vec4<f32>,
    // rgb wet band along the shore, w its half width (m)
    shore: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> look: TerrainLook;

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

fn strata_color(k: i32) -> vec3<f32> {
    let n = max(i32(look.counts.x), 1);
    let i = ((k % n) + n) % n;
    if i == 0 { return look.strata0.rgb; }
    if i == 1 { return look.strata1.rgb; }
    if i == 2 { return look.strata2.rgb; }
    return look.strata3.rgb;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let rel = in.world_position.xyz - look.centre.xyz;
    let r = length(rel);
    let up = rel / max(r, 1.0);
    let h = r - look.centre.w;
    let n = normalize(in.world_normal);
    let slope = degrees(acos(clamp(dot(n, up), -1.0, 1.0)));

#ifdef VERTEX_COLORS
    let ground = in.color.rgb;
    // Below 0 the alpha marks ground under a lake or river (#72; no cap there).
    let cap_share = max(in.color.a, 0.0);
    let inland = clamp(-in.color.a, 0.0, 1.0);
#else
    let ground = vec3(0.5);
    let cap_share = 0.0;
    let inland = 0.0;
#endif
#ifdef VERTEX_UVS_B
    let rock = vec3(in.uv.x, in.uv.y, in.uv_b.x);
    let strata_share = in.uv_b.y;
#else
    let rock = ground;
    let strata_share = 0.0;
#endif

    // Rock on steep faces; a little noise so the line between them is not a contour.
    let wobble = (value_noise(rel * 0.08) - 0.5) * 8.0;
    let rock_t = smoothstep(look.shape.y, look.shape.z, slope + wobble);

    // Strata: bands by height, wobbling a few metres, soft at their borders.
    let band = (h + (value_noise(rel * 0.015) - 0.5) * 2.0 * look.misc.y) / max(look.shape.w, 0.1);
    let k = i32(floor(band));
    let f = fract(band);
    let edge = smoothstep(0.0, 0.08, f) * (1.0 - smoothstep(0.92, 1.0, f));
    let layer = mix(strata_color(k - 1), strata_color(k), edge);
    let rock_col = mix(rock, layer * (rock / max(max(rock.r, max(rock.g, rock.b)), 0.05) * 0.35 + 0.65), strata_share);

    var col = mix(ground, rock_col, rock_t);

    // Cap high up, kept off the steepest faces.
    let above_sea = h - look.shape.x;
    let cap_t = smoothstep(look.cap.w, look.cap.w + look.misc.x, above_sea + wobble) * cap_share * (1.0 - 0.8 * rock_t);
    col = mix(col, look.cap.rgb, cap_t);

    // Grain near the camera: two scales of solid noise, faded out with distance.
    let dist = length(in.world_position.xyz - view.world_position);
    let fade = 1.0 - smoothstep(look.misc.w * 0.4, look.misc.w, dist);
    let grain = value_noise(rel * 0.5) * 0.6 + value_noise(rel * 0.11) * 0.4;
    // A broad patchiness at any distance (20 m), so flats are not one colour.
    let blotch = value_noise(rel * 0.045) - 0.5;
    col = col * (1.0 + (grain - 0.5) * look.misc.z * fade + blotch * look.misc.z * 0.6);

    // Under water the ground turns into the deep colour with depth (shallow lighter, seen
    // through the surface); a wet, lighter band along the shore. Not under a lake or river
    // above the sea: the vertex colour carries its tint.
    let depth = -above_sea;
    col = mix(col, look.water.rgb, smoothstep(0.0, max(look.water.w, 0.1), depth) * (1.0 - inland));
    let shore_t = 1.0 - smoothstep(0.0, max(look.shore.w, 0.01), abs(above_sea - look.shore.w * 0.5));
    col = mix(col, look.shore.rgb, shore_t * 0.6 * (1.0 - inland));

    pbr_input.material.base_color = vec4(col, 1.0);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
