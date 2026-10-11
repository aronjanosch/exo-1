// A planet seen from far away (#224): StandardMaterial lighting with the base colour from an
// equirectangular map of the planet (sea, biome ground, caps), looked up by the direction from
// the planet centre. The map is in planet space: north +y, longitude 0 on +x towards +z
// (planet_core::look::pixel_dir).
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var map_sampler: sampler;

struct ImpostorWater {
    // rgb linear; surface.w the planet radius (m)
    surface: vec4<f32>,
    deep: vec4<f32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> water: ImpostorWater;

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

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // The sphere mesh is in object space around the planet centre: its normal is the direction.
    let d = normalize(in.world_normal);
    let lon = atan2(d.z, d.x);
    let lat = asin(clamp(d.y, -1.0, 1.0));
    let uv = vec2(lon / 6.2831853 + 0.5, 0.5 - lat / 3.1415927);
    let texel = textureSampleLevel(map, map_sampler, uv, 0.0);
    // Water over the land colour where the map's dryness (alpha, 0.5 at a shore) is below one
    // half (#235): the shore line per pixel, frayed by noise on the map's pixel scale (300 m
    // cells) while a cell covers several pixels, so a coast is not a stair step of the map.
    let cells = d * water.surface.w / 300.0;
    let fray = (value_noise(cells) - 0.5) * 0.35 * (1.0 - smoothstep(0.3, 1.0, length(fwidth(cells))));
    let wet = smoothstep(0.47, 0.53, 1.0 - texel.a + fray);
    let sea = mix(water.surface.rgb, water.deep.rgb, smoothstep(0.5, 0.0, texel.a));
    pbr_input.material.base_color = vec4(mix(texel.rgb, sea, wet), 1.0);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
