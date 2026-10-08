//! The terrain material (#66): `StandardMaterial` lighting with the base colour from
//! `content/shaders/terrain.wgsl`. Per vertex the mesh carries the biome palette (ground and
//! cap share in the vertex colour, rock in UV0 + UV1.x, strata share in UV1.y); the planet-wide
//! values (strata bands, cap, detail) and the planet centre in render space are a uniform.
use crate::env::PlanetRes;
use crate::origin::RenderOrigin;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

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
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct TerrainExt {
    #[uniform(100)]
    pub look: TerrainLookUniform,
}

impl MaterialExtension for TerrainExt {
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
fn lin(c: [f32; 3]) -> Vec4 {
    Vec4::new(srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), 1.0)
}

/// The uniform for a planet, with its centre at `centre_render` (render space).
pub fn look_uniform(planet: &PlanetRes, centre_render: Vec3) -> TerrainLookUniform {
    let m = &planet.pgen.recipe.material;
    let s = |i: usize| lin(m.strata_colors[i.min(m.strata_colors.len() - 1)]);
    TerrainLookUniform {
        centre: centre_render.extend(planet.radius as f32),
        shape: Vec4::new(planet.sea as f32, m.rock_slope_deg[0], m.rock_slope_deg[1], m.strata_band_m),
        strata0: s(0),
        strata1: s(1),
        strata2: s(2),
        strata3: s(3),
        cap: lin(m.cap_color).truncate().extend(m.cap_height_m),
        misc: Vec4::new(m.cap_fade_m, m.strata_jitter_m, m.detail_strength, m.detail_far_m),
        counts: Vec4::new(m.strata_colors.len() as f32, 0.0, 0.0, 0.0),
    }
}

pub fn new_material(planet: &PlanetRes, origin: &RenderOrigin) -> TerrainMaterial {
    ExtendedMaterial {
        base: StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.95, ..default() },
        extension: TerrainExt { look: look_uniform(planet, (planet.centre - origin.origin).as_vec3()) },
    }
}

/// Keeps the planet centre in the uniform at its render position (after an origin shift).
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
