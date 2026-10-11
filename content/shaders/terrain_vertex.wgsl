// Terrain vertex stage (#220, geomorphing): each vertex moves towards the position (and normal)
// it has in the parent chunk's mesh by the chunk's morph factor, so a LOD step does not pop: a
// chunk is fully morphed (the parent's shape) when it appears or disappears, and at its own
// detail near the camera. The factor comes per chunk through the mesh tag (0..65535 = 0..1,
// computed on the CPU from the quadtree's own distances, terrain.rs); the per-vertex data are
// the delta to the parent position and the parent's
// normal. The fragment stage is terrain.wgsl. Forward and prepass (depth, shadow) share this.
#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::VertexOutput
#else
#import bevy_pbr::forward_io::VertexOutput
#endif

struct MorphParams {
    // x 1 when morphing is on (F4 toggles it)
    on: vec4<f32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> morph_params: MorphParams;

struct TerrainVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
#ifdef PREPASS_PIPELINE
#ifdef VERTEX_UVS_A
    @location(1) uv: vec2<f32>,
#endif
#ifdef VERTEX_UVS_B
    @location(2) uv_b: vec2<f32>,
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
#ifdef VERTEX_NORMALS
    @location(3) normal: vec3<f32>,
#endif
#endif
#ifdef VERTEX_COLORS
    @location(7) color: vec4<f32>,
#endif
#else
#ifdef VERTEX_NORMALS
    @location(1) normal: vec3<f32>,
#endif
#ifdef VERTEX_UVS_A
    @location(2) uv: vec2<f32>,
#endif
#ifdef VERTEX_UVS_B
    @location(3) uv_b: vec2<f32>,
#endif
#ifdef VERTEX_COLORS
    @location(5) color: vec4<f32>,
#endif
#endif
    @location(10) morph_delta: vec3<f32>,
    @location(11) morph_normal: vec4<f32>,
}

@vertex
fn vertex(v: TerrainVertex) -> VertexOutput {
    var out: VertexOutput;
    let m = f32(mesh_functions::get_tag(v.instance_index)) * (1.0 / 65535.0) * morph_params.on.x;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let local = v.position + v.morph_delta * m;
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(local, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#ifdef VERTEX_UVS_A
    out.uv = v.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = v.uv_b;
#endif
#ifdef VERTEX_COLORS
    out.color = v.color;
#endif
#ifdef VERTEX_NORMALS
#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(normalize(mix(v.normal, v.morph_normal.xyz, m)), v.instance_index);
#endif
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(normalize(mix(v.normal, v.morph_normal.xyz, m)), v.instance_index);
#endif
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.previous_world_position = mesh_functions::mesh_position_local_to_world(
        mesh_functions::get_previous_world_from_local(v.instance_index), vec4<f32>(local, 1.0));
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = v.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(v.instance_index, world_from_local[3]);
#endif
    return out;
}
