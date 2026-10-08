//! Sky, haze and water per planet (#67), from the recipe's `sky` and `water`. The sky is Bevy's
//! `Atmosphere` on an entity at the planet centre (it follows the render origin like the terrain),
//! the camera renders it (`AtmosphereSettings`); the clear colour stays black space. The water is
//! drawn per terrain chunk (`terrain.rs`) with `WaterMaterial`.
use crate::env::PlanetRes;
use crate::origin::{RenderOrigin, WorldPos};
use crate::terrain::PlanetScene;
use crate::terrain_material::lin_pub;
use crate::view::MainCamera;
use bevy::light::atmosphere::{Falloff, PhaseFunction, ScatteringMedium, ScatteringTerm};
use bevy::light::Atmosphere;
use bevy::pbr::{AtmosphereSettings, ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExt>;

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct WaterLookUniform {
    pub centre: Vec4,
    pub surface: Vec4,
    pub ripple: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct WaterExt {
    #[uniform(100)]
    pub look: WaterLookUniform,
}

impl MaterialExtension for WaterExt {
    fn fragment_shader() -> ShaderRef {
        "shaders/water.wgsl".into()
    }
    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/water.wgsl".into()
    }
}

fn water_uniform(planet: &PlanetRes, centre_render: Vec3) -> WaterLookUniform {
    let w = &planet.pgen.recipe.water;
    WaterLookUniform {
        centre: centre_render.extend(0.0),
        surface: lin_pub(w.surface).truncate().extend(w.alpha),
        ripple: Vec4::new(w.ripple_m, w.ripple_speed, 0.0, 0.0),
    }
}

pub fn new_water(planet: &PlanetRes, origin: &RenderOrigin) -> WaterMaterial {
    ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            double_sided: true,
            perceptual_roughness: 0.25,
            ..default()
        },
        extension: WaterExt { look: water_uniform(planet, (planet.centre - origin.origin).as_vec3()) },
    }
}

/// The planet's atmosphere: inner radius at the sea, outer at the atmosphere height.
fn medium(planet: &PlanetRes) -> ScatteringMedium {
    let s = &planet.pgen.recipe.sky;
    let km = 1e-3;
    let v = |c: [f32; 3]| Vec3::new(c[0], c[1], c[2]) * km;
    ScatteringMedium::new(
        256,
        256,
        [
            ScatteringTerm { absorption: Vec3::ZERO, scattering: v(s.rayleigh_per_km), falloff: Falloff::Exponential { scale: s.rayleigh_scale }, phase: PhaseFunction::Rayleigh },
            ScatteringTerm {
                absorption: Vec3::splat(s.mie_absorption_per_km * km),
                scattering: Vec3::splat(s.mie_per_km * km),
                falloff: Falloff::Exponential { scale: s.mie_scale },
                phase: PhaseFunction::Mie { asymmetry: s.mie_asymmetry },
            },
            ScatteringTerm { absorption: v(s.absorption_per_km), scattering: Vec3::ZERO, falloff: Falloff::Tent { center: 0.6, width: 0.4 }, phase: PhaseFunction::Isotropic },
        ],
    )
}

#[derive(Resource, Default)]
pub struct SkyState {
    for_planet: Option<warp_core::PlanetId>,
    entity: Option<Entity>,
}

/// (Re)spawns the atmosphere when the simulation's planet changes; keeps the water uniform's
/// centre at the render position.
#[allow(clippy::too_many_arguments)]
pub fn update_sky(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    origin: Res<RenderOrigin>,
    mut state: ResMut<SkyState>,
    mut media: ResMut<Assets<ScatteringMedium>>,
    terrain: Option<Res<crate::terrain::Terrain>>,
    mut waters: ResMut<Assets<WaterMaterial>>,
) {
    if state.for_planet != Some(planet.id) {
        if let Some(e) = state.entity.take() {
            commands.entity(e).try_despawn();
        }
        let s = &planet.pgen.recipe.sky;
        let atmo = Atmosphere {
            inner_radius: (planet.radius + planet.sea) as f32,
            outer_radius: (planet.radius + planet.field.atmosphere_height) as f32,
            ground_albedo: Vec3::splat(s.ground_albedo),
            medium: media.add(medium(&planet)),
        };
        let e = commands
            .spawn((atmo, Transform::from_translation((planet.centre - origin.origin).as_vec3()), WorldPos(planet.centre), PlanetScene(planet.id)))
            .id();
        state.entity = Some(e);
        state.for_planet = Some(planet.id);
    }
    if let Some(t) = terrain {
        let c = (planet.centre - origin.origin).as_vec3();
        if waters.get(&t.water).is_some_and(|m| m.extension.look.centre.truncate() != c)
            && let Some(mut m) = waters.get_mut(&t.water)
        {
            m.extension.look = water_uniform(&planet, c);
        }
    }
}

/// The main camera renders the atmosphere.
pub fn add_to_camera(mut commands: Commands, cams: Query<Entity, Added<MainCamera>>) {
    for e in &cams {
        commands.entity(e).insert(AtmosphereSettings { aerial_view_lut_max_distance: 20_000.0, ..default() });
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<SkyState>();
    app.add_plugins(MaterialPlugin::<WaterMaterial>::default());
    app.add_systems(Update, (add_to_camera, update_sky.after(crate::terrain::update_terrain)));
}
