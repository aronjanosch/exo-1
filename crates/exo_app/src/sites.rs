use crate::env::{from_v3, PlanetRes};
use crate::origin::{RenderOrigin, WorldPos};
use crate::terrain::PlanetScene;
use bevy::math::DVec3;
use bevy::prelude::*;
use planet_core::recipe::SiteCategory;
use std::collections::HashMap;

/// Sites are shown within this distance of the camera (m): landmarks farther, since they are
/// meant to be seen from afar. TODO(initiator): values.
const SITE_RANGE_M: f64 = 6000.0;
const LANDMARK_RANGE_M: f64 = 20000.0;
const KEEP_FACTOR: f64 = 1.25;
/// The set of sites is checked again after the camera moved this far (m).
const RECHECK_M: f64 = 250.0;

#[derive(Resource, Default)]
pub struct SiteView {
    for_planet: Option<warp_core::PlanetId>,
    meshes: HashMap<String, Handle<Mesh>>,
    materials: HashMap<[u8; 3], Handle<StandardMaterial>>,
    /// Entities of the sites on screen, by site index.
    shown: HashMap<usize, Vec<Entity>>,
    checked_at: Option<DVec3>,
    pub pieces: usize,
}

fn range_of(c: SiteCategory) -> f64 {
    if c == SiteCategory::Landmark { LANDMARK_RANGE_M } else { SITE_RANGE_M }
}

pub fn update_sites(mut commands: Commands, planet: Res<PlanetRes>, origin: Res<RenderOrigin>, assets: Res<AssetServer>, mut view: ResMut<SiteView>, mut mats: ResMut<Assets<StandardMaterial>>) {
    if view.for_planet != Some(planet.id) {
        // The terrain despawns the old planet's scene, these entities with it.
        view.for_planet = Some(planet.id);
        view.shown.clear();
        view.checked_at = None;
        view.pieces = 0;
    }
    let cam = origin.view - planet.centre;
    if view.checked_at.is_some_and(|p| p.distance(cam) < RECHECK_M) {
        return;
    }
    view.checked_at = Some(cam);
    // Distances along the base sphere from the point under the camera.
    let Some(up) = cam.try_normalize() else { return };
    let r = planet.radius;
    let dist = |d: planet_core::V3| from_v3(d).dot(up).clamp(-1.0, 1.0).acos() * r;
    let sites = &planet.pgen.sites;
    let gone: Vec<usize> = view.shown.keys().copied().filter(|&i| dist(sites[i].dir) > range_of(sites[i].category) * KEEP_FACTOR).collect();
    for i in gone {
        for e in view.shown.remove(&i).unwrap_or_default() {
            commands.entity(e).try_despawn();
        }
    }
    let lin = crate::terrain_material::srgb_to_linear;
    for i in 0..sites.len() {
        if view.shown.contains_key(&i) || dist(sites[i].dir) > range_of(sites[i].category) {
            continue;
        }
        let mut entities = Vec::new();
        for p in planet.pgen.site_pieces(i) {
            let [x, u, z] = p.basis.map(|v| from_v3(v).as_vec3());
            let transform = Transform { rotation: Quat::from_mat3(&Mat3::from_cols(x, u, z)), scale: Vec3::splat(p.scale as f32), ..default() };
            let at = WorldPos(planet.centre + from_v3(p.pos));
            if p.prop.contains('/') {
                // A prop id with a '/' names a model under the content root (`city/...`): the
                // whole scene with its own parts and colours, not one tinted primitive.
                let scene = assets.load(GltfAssetLabel::Scene(0).from_asset(format!("{}.glb", p.prop)));
                entities.push(commands.spawn((bevy::world_serialization::WorldAssetRoot(scene), transform, at, PlanetScene(planet.id))).id());
                continue;
            }
            let mesh = view
                .meshes
                .entry(p.prop.clone())
                .or_insert_with(|| assets.load(GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }.from_asset(format!("props/{}.glb", p.prop))))
                .clone();
            let q = p.tint.map(|c| (c.clamp(0.0, 1.0) * 31.0).round() as u8);
            let mat = view
                .materials
                .entry(q)
                .or_insert_with(|| {
                    let c = q.map(|v| lin(v as f32 / 31.0));
                    mats.add(StandardMaterial { base_color: Color::linear_rgb(c[0], c[1], c[2]), perceptual_roughness: 0.85, ..default() })
                })
                .clone();
            entities.push(commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), transform, at, PlanetScene(planet.id))).id());
        }
        view.shown.insert(i, entities);
    }
    view.pieces = view.shown.values().map(Vec::len).sum();
}

pub fn plugin(app: &mut App) {
    app.init_resource::<SiteView>();
    app.add_systems(Update, update_sites.after(crate::terrain::update_terrain).in_set(crate::phases::Frame::World));
}
