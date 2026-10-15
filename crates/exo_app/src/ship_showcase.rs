use crate::env::{from_v3, to_v3, PlanetRes};
use crate::origin::{RenderOrigin, WorldPos};
use crate::terrain::PlanetScene;
use crate::town::Frame;
use crate::Layer;
use avian3d::prelude::*;
use bevy::math::{DAffine3, DQuat, DVec3};
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};

/// The models: asset path, spot east and north of the spawn (m), turn (deg). The flyable ship
/// parks 15 m ahead of the spawn. TODO(initiator): spots.
const MODELS: [(&str, f64, f64, f64); 4] = [
    ("ship/starting-ship-study.glb", 30.0, 0.0, 0.0),
    ("local/pulsar/full/Assets/Resources/networkprefabs/CivilianStartingShip.glb", -80.0, 0.0, 0.0),
    ("local/pulsar/full/Assets/Resources/networkprefabs/Intrepid.glb", -190.0, 0.0, 0.0),
    ("local/pulsar/full/Assets/Resources/networkprefabs/Roland.glb", 200.0, 0.0, 0.0),
];

/// In the foreign PULSAR prefabs (`local/`), nodes whose subtree is neither drawn nor collided:
/// helper and effect meshes (shadow casters, flight rings, the shield bubble, the planes of the
/// world-space screens) and the outside hull,
/// which sits apart from the larger interior. Own models keep everything (Bevy names the scene
/// root after the Blender scene, which may contain these words).
const SKIP: [&str; 6] = ["ShadowCaster", "FlightRing", "SheildBubble", "Exterior", "Shield", "WorldRoot"];

const LIFT: f64 = 0.05;

#[derive(Resource, Clone, Copy)]
pub struct ShowcaseStart(pub DVec3);

/// On a showcase root once its collider stands: the model origin (world) and the largest
/// horizontal distance of a vertex from it (m).
#[derive(Component, Clone, Copy, Debug)]
pub struct ShowcaseReady {
    pub origin: DVec3,
    pub radius: f64,
}

#[derive(Component)]
pub struct Showcase {
    pub path: &'static str,
    pub ground: DVec3,
    pub up: DVec3,
    pub rot: DQuat,
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_showcase);
    app.add_observer(on_ready);
}

fn spawn_showcase(mut commands: Commands, planet: Res<PlanetRes>, assets: Res<AssetServer>, start: Res<ShowcaseStart>) {
    let dir = start.0;
    let (e, n) = planet_core::look::tangent_frame(to_v3(dir));
    let frame = Frame { dir, east: from_v3(e), north: from_v3(n) };
    let root = std::path::PathBuf::from(crate::content_dir());
    for (path, x, y, turn) in MODELS {
        if !root.join(path).exists() {
            println!("showcase: {path} missing, skipped");
            continue;
        }
        let (ground, up) = frame.ground(&planet, x, y, 0.0);
        let rot = frame.rotation(up, turn);
        commands.spawn((
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(path))),
            Transform::from_rotation(rot.as_quat()),
            WorldPos(ground),
            PlanetScene(planet.id),
            Showcase { path, ground, up, rot },
        ));
    }
}

fn skipped(foreign: bool, name: Option<&Name>) -> bool {
    foreign && name.is_some_and(|n| SKIP.iter().any(|s| n.as_str().contains(s)))
}

fn affine(t: &Transform) -> DAffine3 {
    DAffine3::from_scale_rotation_translation(t.scale.as_dvec3(), t.rotation.as_dquat(), t.translation.as_dvec3())
}

#[allow(clippy::too_many_arguments)]
fn on_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    shows: Query<&Showcase>,
    children: Query<&Children>,
    nodes: Query<(&Transform, Option<&Name>, Option<&Mesh3d>)>,
    meshes: Res<Assets<Mesh>>,
    origin: Res<RenderOrigin>,
) {
    let root = ready.entity;
    let Ok(show) = shows.get(root) else { return };
    let foreign = show.path.starts_with("local/");
    let mut verts: Vec<DVec3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut stack: Vec<(Entity, DAffine3)> = children.get(root).map(|c| c.iter().map(|e| (e, DAffine3::IDENTITY)).collect()).unwrap_or_default();
    while let Some((e, parent)) = stack.pop() {
        let Ok((t, name, mesh)) = nodes.get(e) else {
            if let Ok(c) = children.get(e) {
                stack.extend(c.iter().map(|k| (k, parent)));
            }
            continue;
        };
        if skipped(foreign, name) {
            commands.entity(e).insert(Visibility::Hidden);
            continue;
        }
        let at = parent * affine(t);
        if let Some(m) = mesh.and_then(|m| meshes.get(&m.0)) {
            let pos = match m.try_attribute_option(Mesh::ATTRIBUTE_POSITION) {
                Ok(Some(VertexAttributeValues::Float32x3(p))) => p,
                _ => {
                    stack.extend(children.get(e).map(|c| c.iter().map(|k| (k, at)).collect::<Vec<_>>()).unwrap_or_default());
                    continue;
                }
            };
            let base = verts.len() as u32;
            verts.extend(pos.iter().map(|p| at.transform_point3(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))));
            let idx: Vec<u32> = match m.indices() {
                Some(Indices::U16(i)) => i.iter().map(|&i| i as u32).collect(),
                Some(Indices::U32(i)) => i.clone(),
                None => (0..pos.len() as u32).collect(),
            };
            tris.extend(idx.as_chunks::<3>().0.iter().map(|c| [base + c[0], base + c[1], base + c[2]]));
        }
        if let Ok(c) = children.get(e) {
            stack.extend(c.iter().map(|k| (k, at)));
        }
    }
    if tris.is_empty() {
        println!("showcase: {}: no meshes", show.path);
        return;
    }
    let min_y = verts.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
    let radius = verts.iter().map(|v| v.x.hypot(v.z)).fold(0.0, f64::max);
    let at = show.ground + show.up * (LIFT - min_y);
    commands.entity(root).insert((WorldPos(at), Transform::from_translation((at - origin.origin).as_vec3()).with_rotation(show.rot.as_quat())));
    let n = tris.len();
    match Collider::try_trimesh(verts, tris) {
        Ok(collider) => {
            commands.spawn((RigidBody::Static, collider, Position(at), Rotation(show.rot), CollisionLayers::new(Layer::World, LayerMask::ALL)));
            commands.entity(root).insert(ShowcaseReady { origin: at, radius });
            println!("showcase: {}: {n} triangles, lifted {:.2} m", show.path, -min_y);
        }
        Err(e) => println!("showcase: {}: no collider ({e:?})", show.path),
    }
}
