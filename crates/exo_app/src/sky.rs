//! Sky, haze and water per planet, from the recipe's `sky` and `water`. The sky is Bevy's
//! `Atmosphere` on an entity at the planet centre (it follows the render origin like the terrain),
//! the camera renders it (`AtmosphereSettings`); the clear colour stays black space. The water is
//! drawn per terrain chunk (`terrain.rs`) with `WaterMaterial`. Every density tuned at the 1.2 km
//! reference atmosphere scales with the planet's atmosphere height (`atmo_scale`). Far
//! away the planet is the impostor sphere with a colour map baked from the recipe
//! (`ImpostorMaterial`), under a cloud shell (`CloudMaterial`, recipe `sky.clouds`).
use crate::env::PlanetRes;
use crate::origin::{RenderOrigin, WorldPos, WorldPose};
use crate::terrain::PlanetScene;
use crate::terrain_material::lin_pub;
use crate::view::MainCamera;
use bevy::light::atmosphere::{Falloff, PhaseFunction, ScatteringMedium, ScatteringTerm};
use bevy::light::Atmosphere;
use bevy::math::DVec3;
use bevy::pbr::{AtmosphereSettings, ExtendedMaterial, MaterialExtension};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};

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

/// The atmosphere height the recipe's per-km and per-metre sky values are tuned for (Hearth at
/// 5 km radius).
pub const REFERENCE_ATMOSPHERE_M: f64 = 1200.0;

/// Dev switch for before/after shots: `EXO_SKY_LEGACY` brings back the 5 km tuning, no clouds and
/// the flat impostor.
fn legacy() -> bool {
    std::env::var("EXO_SKY_LEGACY").is_ok()
}

/// Factor for every density tuned at the reference atmosphere (scattering, haze): a planet with
/// an atmosphere `k` times as high gets `1/k` of the density per metre, so the optical depth
/// straight up stays the same and the ground stays readable over the longer distances.
pub fn atmo_scale(planet: &PlanetRes) -> f32 {
    if legacy() {
        return 1.0;
    }
    (REFERENCE_ATMOSPHERE_M / planet.field.atmosphere_height.max(1.0)) as f32
}

/// The haze density per metre of this planet (before the time of day): the recipe's value scaled
/// to the planet's atmosphere height.
pub fn haze_density(planet: &PlanetRes) -> f32 {
    planet.pgen.recipe.sky.haze_density * atmo_scale(planet)
}

/// The planet's atmosphere: inner radius at the sea, outer at the atmosphere height.
fn medium(planet: &PlanetRes) -> ScatteringMedium {
    let s = &planet.pgen.recipe.sky;
    let km = 1e-3 * atmo_scale(planet);
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

/// The aerial view reaches 20 km at the reference atmosphere and grows with the atmosphere
/// height (the horizon is at about the square root of radius times height).
fn aerial_distance(planet: &PlanetRes) -> f32 {
    if legacy() {
        return 20_000.0;
    }
    (20_000.0 * planet.field.atmosphere_height / REFERENCE_ATMOSPHERE_M) as f32
}

/// The main camera renders the atmosphere.
pub fn add_to_camera(mut commands: Commands, cams: Query<Entity, Added<MainCamera>>, planet: Res<PlanetRes>) {
    for e in &cams {
        commands.entity(e).insert(AtmosphereSettings { aerial_view_lut_max_distance: aerial_distance(&planet), ..default() });
    }
}

/// Keeps the aerial view's reach at the planet's atmosphere (a planet swap changes it).
pub fn tune_atmosphere(planet: Res<PlanetRes>, mut cams: Query<&mut AtmosphereSettings, With<MainCamera>>) {
    if !planet.is_changed() {
        return;
    }
    for mut a in &mut cams {
        a.aerial_view_lut_max_distance = aerial_distance(&planet);
    }
}


// ---- Impostor colour map ----

/// Pixels of the equirectangular colour map of the planet seen from far away (width; the height
/// is half of it).
const MAP_WIDTH: usize = 1024;

pub type ImpostorMaterial = ExtendedMaterial<StandardMaterial, ImpostorExt>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct ImpostorExt {
    #[texture(100)]
    #[sampler(101)]
    pub map: Handle<Image>,
    #[uniform(102)]
    pub water: ImpostorWater,
}

/// The water the impostor shader draws over the map's dryness: linear colours, and in
/// `surface.w` the planet's radius (m, for the shore's noise).
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct ImpostorWater {
    pub surface: Vec4,
    pub deep: Vec4,
}

impl MaterialExtension for ImpostorExt {
    fn fragment_shader() -> ShaderRef {
        "shaders/impostor.wgsl".into()
    }
}

/// The map's bake runs on a task; when it is done the planet's impostor gets it.
#[derive(Resource, Default)]
pub struct ImpostorMapState {
    for_planet: Option<warp_core::PlanetId>,
    task: Option<Task<Vec<u8>>>,
}

pub fn update_impostor_map(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    mut state: ResMut<ImpostorMapState>,
    impostors: Query<(Entity, &crate::view::Impostor)>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<ImpostorMaterial>>,
) {
    if legacy() {
        return;
    }
    if state.for_planet != Some(planet.id) {
        state.for_planet = Some(planet.id);
        let pgen = planet.pgen.clone();
        state.task = Some(AsyncComputeTaskPool::get().spawn(async move { pgen.orbit_map(MAP_WIDTH, 2) }));
    }
    let Some(task) = state.task.as_mut() else { return };
    let Some(rgba) = block_on(future::poll_once(task)) else { return };
    state.task = None;
    let mut img = Image::new(
        Extent3d { width: MAP_WIDTH as u32, height: (MAP_WIDTH / 2) as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: ImageAddressMode::Repeat, ..ImageSamplerDescriptor::linear() });
    let map = images.add(img);
    let mat = mats.add(ExtendedMaterial {
        base: StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.9, ..default() },
        extension: ImpostorExt {
            map,
            water: ImpostorWater {
                surface: crate::terrain_material::lin_pub(planet.pgen.recipe.water.surface).truncate().extend(planet.radius as f32),
                deep: crate::terrain_material::lin_pub(planet.pgen.recipe.water.deep),
            },
        },
    });
    for (e, i) in &impostors {
        if i.0 == planet.id {
            commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert(MeshMaterial3d(mat.clone()));
        }
    }
}

// ---- Clouds ----

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct CloudLookUniform {
    /// rgb colour, w coverage.
    pub color: Vec4,
    /// xyz towards the sun, w noise cells per radian.
    pub sun: Vec4,
    /// x drift (cells per second), y and z: the distance where the cloud starts to show and
    /// where it is full (m).
    pub misc: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct CloudMaterial {
    #[uniform(0)]
    pub look: CloudLookUniform,
}

impl Material for CloudMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/clouds.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Component)]
pub struct CloudShell {
    radius: f64,
    material: Handle<CloudMaterial>,
}

/// Radius from the planet centre beyond which the planet is drawn as the impostor sphere: two
/// atmosphere heights above the surface (the arrival radius's rule), never before `IMPOSTOR_FROM`.
pub fn impostor_from(radius: f64, atmosphere_height: f64) -> f64 {
    if legacy() {
        return crate::view::IMPOSTOR_FROM;
    }
    crate::view::IMPOSTOR_FROM.max(radius + 2.0 * atmosphere_height)
}

fn cloud_uniform(planet: &PlanetRes, sun: DVec3) -> Option<(CloudLookUniform, f64)> {
    let c = planet.pgen.recipe.sky.clouds.as_ref().filter(|c| !legacy() && planet.radius >= c.min_radius_m)?;
    let h = planet.field.atmosphere_height;
    let radius = planet.radius + c.altitude as f64 * h;
    let cells = (radius / c.scale_m.max(1.0) as f64).max(2.5) as f32;
    let col = lin_pub(c.color);
    let s = sun.as_vec3();
    Some((
        CloudLookUniform {
            color: Vec4::new(col.x, col.y, col.z, c.coverage),
            sun: s.extend(cells),
            misc: Vec4::new(c.drift_m_s / c.scale_m.max(1.0), (0.03 * h) as f32, (0.2 * h) as f32, 0.0),
        },
        radius,
    ))
}

/// (Re)spawns the cloud shell when the planet changes; none on a planet without `sky.clouds`.
#[derive(Resource, Default)]
pub struct CloudState {
    for_planet: Option<warp_core::PlanetId>,
    entity: Option<Entity>,
}

#[allow(clippy::too_many_arguments)]
pub fn update_clouds(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    origin: Res<RenderOrigin>,
    sun: Res<crate::daynight::Sun>,
    mut state: ResMut<CloudState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<CloudMaterial>>,
    mut shells: Query<(&CloudShell, &mut WorldPose, &mut Transform)>,
) {
    if state.for_planet != Some(planet.id) {
        if let Some(e) = state.entity.take() {
            commands.entity(e).try_despawn();
        }
        state.for_planet = Some(planet.id);
        if let Some((look, radius)) = cloud_uniform(&planet, sun.dir) {
            let material = mats.add(CloudMaterial { look });
            let e = commands
                .spawn((
                    CloudShell { radius, material: material.clone() },
                    Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(96, 48))),
                    MeshMaterial3d(material),
                    Transform::default(),
                    WorldPose::default(),
                    NotShadowCaster,
                    NotShadowReceiver,
                    PlanetScene(planet.id),
                ))
                .id();
            state.entity = Some(e);
        }
        return;
    }
    let Ok((shell, mut pose, mut t)) = shells.single_mut() else { return };
    // Like the impostor: far from the planet the shell keeps its angular size at a fixed distance.
    let to = planet.centre - origin.view;
    let dist = to.length();
    let from = impostor_from(planet.radius, planet.field.atmosphere_height);
    let (at, scale) = if dist > from {
        let shown = crate::view::IMPOSTOR_FROM;
        (origin.view + to / dist * shown, shell.radius * shown / dist)
    } else {
        (planet.centre, shell.radius)
    };
    pose.pos = at;
    t.scale = Vec3::splat(scale as f32);
    if let Some((look, _)) = cloud_uniform(&planet, sun.dir)
        && mats.get(&shell.material).is_some_and(|m| m.look.sun != look.sun)
        && let Some(mut m) = mats.get_mut(&shell.material)
    {
        m.look.sun = look.sun;
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<SkyState>().init_resource::<ImpostorMapState>().init_resource::<CloudState>();
    app.add_plugins((MaterialPlugin::<WaterMaterial>::default(), MaterialPlugin::<ImpostorMaterial>::default(), MaterialPlugin::<CloudMaterial>::default()));
    app.add_systems(
        Update,
        (add_to_camera, tune_atmosphere, update_sky.after(crate::terrain::update_terrain), update_impostor_map, update_clouds).in_set(crate::phases::Frame::World),
    );
}
