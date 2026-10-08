//! Sites on screen (#70): every site's kit pieces (`planet_core::site_pieces`, props from
//! `art/props/props.py`), spawned once per planet and despawned with its scene. About 30 sites
//! of up to 25 pieces each: no streaming needed.
use crate::env::{from_v3, PlanetRes};
use crate::origin::WorldPos;
use crate::terrain::PlanetScene;
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct SiteView {
    for_planet: Option<warp_core::PlanetId>,
    meshes: HashMap<String, Handle<Mesh>>,
    materials: HashMap<[u8; 3], Handle<StandardMaterial>>,
    pub pieces: usize,
}

pub fn update_sites(mut commands: Commands, planet: Res<PlanetRes>, assets: Res<AssetServer>, mut view: ResMut<SiteView>, mut mats: ResMut<Assets<StandardMaterial>>) {
    if view.for_planet == Some(planet.id) {
        return;
    }
    view.for_planet = Some(planet.id);
    view.pieces = 0;
    let lin = crate::terrain_material::srgb_to_linear;
    for i in 0..planet.pgen.sites.len() {
        for p in planet.pgen.site_pieces(i) {
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
            let [x, u, z] = p.basis.map(|v| from_v3(v).as_vec3());
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform { rotation: Quat::from_mat3(&Mat3::from_cols(x, u, z)), scale: Vec3::splat(p.scale as f32), ..default() },
                WorldPos(planet.centre + from_v3(p.pos)),
                PlanetScene,
            ));
            view.pieces += 1;
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<SiteView>();
    app.add_systems(Update, update_sites.after(crate::terrain::update_terrain));
}
