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

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // The sphere mesh is in object space around the planet centre: its normal is the direction.
    let d = normalize(in.world_normal);
    let lon = atan2(d.z, d.x);
    let lat = asin(clamp(d.y, -1.0, 1.0));
    let uv = vec2(lon / 6.2831853 + 0.5, 0.5 - lat / 3.1415927);
    pbr_input.material.base_color = vec4(textureSampleLevel(map, map_sampler, uv, 0.0).rgb, 1.0);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
