use crate::env::PlanetRes;
use crate::origin::RenderOrigin;
use bevy::mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef, VertexFormat};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

/// Geomorphing: the delta to the vertex's position in the parent chunk's mesh, and the
/// parent's normal (see `terrain_vertex.wgsl`).
pub const ATTRIBUTE_MORPH_DELTA: MeshVertexAttribute = MeshVertexAttribute::new("MorphDelta", 0x4c34_0a01, VertexFormat::Float32x3);
pub const ATTRIBUTE_MORPH_NORMAL: MeshVertexAttribute = MeshVertexAttribute::new("MorphNormal", 0x4c34_0a02, VertexFormat::Snorm8x4);

/// Per material: whether the vertex stage morphs (F4 in the window; x of the vec).
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct MorphUniform {
    pub on: Vec4,
}

impl Default for MorphUniform {
    fn default() -> Self {
        // x: on (F4), y and z: the morph range in chunk edges (`terrain::MORPH_NEAR`, `MORPH_FAR`).
        MorphUniform { on: Vec4::new(f32::from(morph_default()), crate::terrain::MORPH_NEAR as f32, crate::terrain::MORPH_FAR as f32, 0.0) }
    }
}

/// Geomorphing starts on, off with `EXO_NO_MORPH` set.
pub fn morph_default() -> bool {
    std::env::var("EXO_NO_MORPH").is_err()
}

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExt>;

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct TerrainLookUniform {
    pub centre: Vec4,
    pub shape: Vec4,
    pub strata0: Vec4,
    pub strata1: Vec4,
    pub strata2: Vec4,
    pub strata3: Vec4,
    pub cap: Vec4,
    pub misc: Vec4,
    pub counts: Vec4,
    pub water: Vec4,
    pub shore: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct TerrainExt {
    #[uniform(100)]
    pub look: TerrainLookUniform,
    #[uniform(101)]
    pub morph: MorphUniform,
}

impl MaterialExtension for TerrainExt {
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
    fn vertex_shader() -> ShaderRef {
        "shaders/terrain_vertex.wgsl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "shaders/terrain_vertex.wgsl".into()
    }
    fn deferred_vertex_shader() -> ShaderRef {
        "shaders/terrain_vertex.wgsl".into()
    }
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The morph attributes join the layout the base material built (one interleaved buffer).
        let extra = layout.0.get_layout(&[ATTRIBUTE_MORPH_DELTA.at_shader_location(10), ATTRIBUTE_MORPH_NORMAL.at_shader_location(11)])?;
        descriptor.vertex.buffers[0].attributes.extend(extra.attributes);
        Ok(())
    }
}

pub fn lin_pub(c: [f32; 3]) -> Vec4 {
    lin(c)
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
fn lin(c: [f32; 3]) -> Vec4 {
    Vec4::new(srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), 1.0)
}

pub fn look_uniform(planet: &PlanetRes, centre_render: Vec3) -> TerrainLookUniform {
    let m = &planet.pgen.recipe.material;
    let s = |i: usize| lin(m.strata_colors[i.min(m.strata_colors.len() - 1)]);
    // The recipe's metres are for the radius it was tuned on; a taller relief stretches them.
    let k = planet.pgen.look_scale() as f32;
    TerrainLookUniform {
        centre: centre_render.extend(planet.radius as f32),
        shape: Vec4::new(planet.sea as f32, m.rock_slope_deg[0], m.rock_slope_deg[1], m.strata_band_m * k),
        strata0: s(0),
        strata1: s(1),
        strata2: s(2),
        strata3: s(3),
        cap: lin(m.cap_color).truncate().extend(m.cap_height_m * k),
        misc: Vec4::new(m.cap_fade_m * k, m.strata_jitter_m * k, m.detail_strength, m.detail_far_m),
        counts: Vec4::new(m.strata_colors.len() as f32, 0.0, 0.0, 0.0),
        water: lin(planet.pgen.recipe.water.deep).truncate().extend(planet.pgen.recipe.water.depth_m),
        shore: lin(planet.pgen.recipe.water.shore).truncate().extend(planet.pgen.recipe.water.shore_width_m),
    }
}

pub fn new_material(planet: &PlanetRes, origin: &RenderOrigin) -> TerrainMaterial {
    ExtendedMaterial {
        base: StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.95, ..default() },
        extension: TerrainExt { look: look_uniform(planet, (planet.centre - origin.origin).as_vec3()), morph: MorphUniform::default() },
    }
}

pub fn follow_origin(
    origin: Res<RenderOrigin>,
    planet: Res<PlanetRes>,
    terrain: Option<Res<crate::terrain::Terrain>>,
    mut mats: ResMut<Assets<TerrainMaterial>>,
) {
    let Some(t) = terrain else { return };
    let c = (planet.centre - origin.origin).as_vec3();
    let Some(m) = mats.get(&t.material) else { return };
    if m.extension.look.centre.truncate() != c || m.extension.look.centre.w != planet.radius as f32 {
        if let Some(mut m) = mats.get_mut(&t.material) {
            m.extension.look = look_uniform(&planet, c);
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<TerrainMaterial>::default());
    app.add_systems(PostUpdate, follow_origin.after(TransformSystems::Propagate));
}
