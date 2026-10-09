//! Cube-sphere terrain with a quadtree LOD per face. Chunks come from
//! planet_core::build_chunk on the async compute pool; the main thread uploads a few per frame.
//! View only: collision comes from the ring.
use crate::env::{from_v3, PlanetRes};
use crate::origin::{RenderOrigin, WorldPos};
use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use planet_core::{cube_to_sphere, ChunkOut, M};

const SPLIT_FACTOR: f64 = 1.5;
const MERGE_FACTOR: f64 = 1.8;
const MAX_UPLOADS_PER_FRAME: usize = 4;

struct Node {
    face: usize,
    a0: f64,
    b0: f64,
    size: f64,
    depth: u32,
    centre: DVec3, // planet space, base sphere
    edge_m: f64,
    bound: f64,
    children: Vec<usize>,
    entity: Option<Entity>,
    mesh: Option<AssetId<Mesh>>,
    water_mesh: Option<AssetId<Mesh>>,
    /// Lakes and rivers (#72).
    inland_mesh: Option<AssetId<Mesh>>,
    task: Option<Task<ChunkOut>>,
    alive: bool,
}

/// Everything the terrain spawns for one planet (chunks, water, markers): despawned together
/// when the simulation's planet changes (#14: refined chunks used to miss it and stayed behind).
#[derive(Component)]
pub struct PlanetScene(pub warp_core::PlanetId);

/// A terrain chunk (root or refined) of a planet.
#[derive(Component)]
pub struct TerrainChunk;

/// The six root chunks of a planet, built on the pool while the ship flies there (#34), so the
/// frame of the planet swap does not build them.
#[derive(Resource)]
pub struct PrebuiltRoots {
    pub planet: warp_core::PlanetId,
    pub chunks: Vec<ChunkOut>,
}

/// The root chunk of each cube face.
const ROOT: (f64, f64, f64) = (-1.0, -1.0, 2.0);

/// The six root chunks of `planet` (also called on the pool, see `PrebuiltRoots`).
pub fn build_roots(planet: &PlanetRes) -> Vec<ChunkOut> {
    (0..6).map(|face| planet.pgen.build_chunk(face, ROOT.0, ROOT.1, ROOT.2)).collect()
}

#[derive(Resource)]
pub struct Terrain {
    /// Registry id of the planet these chunks belong to.
    pub for_planet: warp_core::PlanetId,
    nodes: Vec<Node>,
    free: Vec<usize>,
    roots: Vec<usize>,
    max_depth: u32,
    pub material: Handle<crate::terrain_material::TerrainMaterial>,
    pub water: Handle<crate::sky::WaterMaterial>,
    indices: Vec<u32>,
    pub visible: usize,
    pub pending: usize,
    /// Root chunks built on the main thread when this terrain was built (6 without prebuilt roots).
    pub roots_built_here: usize,
}

use crate::terrain_material::{srgb_to_linear, TerrainMaterial};

impl Terrain {
    fn make_node(&mut self, planet: &PlanetRes, face: usize, a0: f64, b0: f64, size: f64, depth: u32) -> usize {
        let r = planet.radius;
        let centre = from_v3(cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5)) * r;
        let edge_m = (from_v3(cube_to_sphere(face, a0, b0)) - from_v3(cube_to_sphere(face, a0 + size, b0))).length() * r;
        let mut bound: f64 = 0.0;
        for (a, b) in [(a0, b0), (a0 + size, b0), (a0, b0 + size), (a0 + size, b0 + size)] {
            bound = bound.max(centre.distance(from_v3(cube_to_sphere(face, a, b)) * r));
        }
        let node = Node {
            face, a0, b0, size, depth, centre, edge_m,
            bound: bound + planet.relief,
            children: Vec::new(), entity: None, mesh: None, water_mesh: None, inland_mesh: None, task: None, alive: true,
        };
        if let Some(i) = self.free.pop() {
            self.nodes[i] = node;
            i
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        }
    }

    fn start(&mut self, planet: &PlanetRes, i: usize) {
        let n = &self.nodes[i];
        let (face, a0, b0, size) = (n.face, n.a0, n.b0, n.size);
        let pgen = planet.pgen.clone();
        self.nodes[i].task = Some(AsyncComputeTaskPool::get().spawn(async move { pgen.build_chunk(face, a0, b0, size) }));
    }

    fn discard(&mut self, commands: &mut Commands, i: usize) {
        let children = std::mem::take(&mut self.nodes[i].children);
        for c in children {
            self.discard(commands, c);
        }
        let n = &mut self.nodes[i];
        if let Some(e) = n.entity.take() {
            commands.entity(e).despawn();
        }
        n.mesh = None;
        n.task = None; // dropping a bevy Task cancels it
        n.alive = false;
        self.free.push(i);
    }

    fn mesh(&self, out: &ChunkOut) -> Mesh {
        // The biome palette (#66): ground + cap share as the colour, rock in UV0 + UV1.x, strata
        // share in UV1.y (see terrain_material.rs).
        let colors: Vec<[f32; 4]> = out.colors.iter().map(|c| [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), c[3]]).collect();
        let uv0: Vec<[f32; 2]> = out.rock.iter().map(|c| [srgb_to_linear(c[0]), srgb_to_linear(c[1])]).collect();
        let uv1: Vec<[f32; 2]> = out.rock.iter().map(|c| [srgb_to_linear(c[2]), c[3]]).collect();
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, out.verts.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, out.normals.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv0)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1)
            .with_inserted_indices(Indices::U32(self.indices.clone()))
    }

    /// Every mesh this terrain holds (chunks and their water), for the swap check (#14).
    pub fn mesh_ids(&self) -> Vec<AssetId<Mesh>> {
        self.nodes.iter().filter(|n| n.alive).flat_map(|n| n.mesh.into_iter().chain(n.water_mesh).chain(n.inland_mesh)).collect()
    }

    fn water_mesh(&self, out: &ChunkOut) -> Option<Mesh> {
        let w = out.water.as_ref()?;
        let c = DVec3::from_array(out.center);
        let normals: Vec<[f32; 3]> = w.iter().map(|p| (c + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)).normalize().as_vec3().to_array()).collect();
        Some(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, w.clone())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
                .with_inserted_indices(Indices::U32(self.indices.clone())),
        )
    }

    /// Lakes and rivers over the chunk (#72): their own vertices and triangles, the sea's
    /// material.
    fn inland_water_mesh(out: &ChunkOut) -> Option<Mesh> {
        let (w, tris) = out.inland_water.as_ref()?;
        let c = DVec3::from_array(out.center);
        let normals: Vec<[f32; 3]> = w.iter().map(|p| (c + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)).normalize().as_vec3().to_array()).collect();
        Some(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, w.clone())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
                .with_inserted_indices(Indices::U32(tris.clone())),
        )
    }

    /// The chunk's sea, lake and river surfaces as children of the chunk entity (shown and
    /// dropped with it).
    fn spawn_water(&mut self, commands: &mut Commands, meshes: &mut Assets<Mesh>, i: usize, chunk: Entity, out: &ChunkOut) {
        if let Some(m) = self.water_mesh(out) {
            let mesh = meshes.add(m);
            self.nodes[i].water_mesh = Some(mesh.id());
            commands.spawn((Mesh3d(mesh), MeshMaterial3d(self.water.clone()), Transform::default(), ChildOf(chunk)));
        }
        if let Some(m) = Self::inland_water_mesh(out) {
            let mesh = meshes.add(m);
            self.nodes[i].inland_mesh = Some(mesh.id());
            commands.spawn((Mesh3d(mesh), MeshMaterial3d(self.water.clone()), Transform::default(), ChildOf(chunk)));
        }
    }

    /// Returns true when this node (or its children) is on screen.
    fn update_node(&mut self, commands: &mut Commands, planet: &PlanetRes, i: usize, cam: DVec3) {
        let n = &self.nodes[i];
        let dist = (cam.distance(n.centre) - n.bound).max(0.0);
        let factor = if n.children.is_empty() { SPLIT_FACTOR } else { MERGE_FACTOR };
        let want_split = n.depth < self.max_depth && dist < n.edge_m * factor;
        if want_split {
            if self.nodes[i].children.is_empty() {
                let (face, a0, b0, half, depth) = (n.face, n.a0, n.b0, n.size * 0.5, n.depth + 1);
                let mut kids = Vec::with_capacity(4);
                for j in 0..2 {
                    for k in 0..2 {
                        let c = self.make_node(planet, face, a0 + k as f64 * half, b0 + j as f64 * half, half, depth);
                        self.start(planet, c);
                        kids.push(c);
                    }
                }
                self.nodes[i].children = kids;
            }
            let kids = self.nodes[i].children.clone();
            if kids.iter().all(|&c| self.nodes[c].entity.is_some()) {
                if let Some(e) = self.nodes[i].entity {
                    commands.entity(e).insert(Visibility::Hidden);
                }
                for c in kids {
                    self.update_node(commands, planet, c, cam);
                }
                return;
            }
            // Children still building: keep the parent on screen, no holes.
        } else if !self.nodes[i].children.is_empty() {
            for c in std::mem::take(&mut self.nodes[i].children) {
                self.discard(commands, c);
            }
        }
        if let Some(e) = self.nodes[i].entity {
            commands.entity(e).insert(Visibility::Inherited);
        }
        self.visible += 1;
    }
}

pub fn setup_terrain(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    origin: Res<RenderOrigin>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain_mats: ResMut<Assets<TerrainMaterial>>,
    mut waters: ResMut<Assets<crate::sky::WaterMaterial>>,
) {
    let material = terrain_mats.add(crate::terrain_material::new_material(&planet, &origin));
    let water = waters.add(crate::sky::new_water(&planet, &origin));
    let t = build_terrain(&mut commands, &planet, &mut meshes, material, water, None);
    commands.insert_resource(t);
}

/// Root chunks (prebuilt, or here and now so there is always a planet), water and site markers
/// of `planet`.
pub fn build_terrain(
    commands: &mut Commands,
    planet: &PlanetRes,
    meshes: &mut Assets<Mesh>,
    material: Handle<TerrainMaterial>,
    water: Handle<crate::sky::WaterMaterial>,
    prebuilt: Option<Vec<ChunkOut>>,
) -> Terrain {
    let face_edge = planet.radius * std::f64::consts::PI * 0.5;
    let mut indices = Vec::with_capacity((M - 1) * (M - 1) * 6);
    for j in 0..M - 1 {
        for i in 0..M - 1 {
            let k00 = (j * M + i) as u32;
            let (k10, k01) = (k00 + 1, k00 + M as u32);
            let k11 = k01 + 1;
            // Counter-clockwise front faces (Bevy).
            indices.extend([k00, k10, k01, k10, k11, k01]);
        }
    }
    let mut t = Terrain {
        for_planet: planet.id,
        nodes: Vec::new(), free: Vec::new(), roots: Vec::new(),
        max_depth: ((face_edge / 37.0).log2().round() as u32).max(1),
        material, water, indices,
        visible: 0, pending: 0, roots_built_here: 0,
    };
    // Roots synchronously (or prebuilt on the pool, #34), so there is always a planet.
    let roots = prebuilt.filter(|r| r.len() == 6).unwrap_or_else(|| {
        t.roots_built_here = 6;
        build_roots(planet)
    });
    for (face, out) in roots.into_iter().enumerate() {
        let i = t.make_node(planet, face, ROOT.0, ROOT.1, ROOT.2, 0);
        let mesh = meshes.add(t.mesh(&out));
        t.nodes[i].mesh = Some(mesh.id());
        let e = commands
            .spawn((Mesh3d(mesh), MeshMaterial3d(t.material.clone()), Transform::default(), WorldPos(planet.centre + DVec3::from_array(out.center)), PlanetScene(planet.id), TerrainChunk))
            .id();
        t.spawn_water(commands, meshes, i, e, &out);
        t.nodes[i].entity = Some(e);
        t.roots.push(i);
    }
    t
}

#[allow(clippy::too_many_arguments)]
pub fn update_terrain(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    origin: Res<RenderOrigin>,
    mut terrain: ResMut<Terrain>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain_mats: ResMut<Assets<TerrainMaterial>>,
    mut waters: ResMut<Assets<crate::sky::WaterMaterial>>,
    scene: Query<Entity, With<PlanetScene>>,
    prebuilt: Option<ResMut<PrebuiltRoots>>,
) {
    if terrain.for_planet != planet.id {
        // The simulation's planet changed (warp or teleport): free the old terrain, build the new.
        for e in &scene {
            commands.entity(e).despawn();
        }
        let material = terrain.material.clone();
        if let Some(mut m) = terrain_mats.get_mut(&material) {
            *m = crate::terrain_material::new_material(&planet, &origin);
        }
        let water = terrain.water.clone();
        if let Some(mut m) = waters.get_mut(&water) {
            *m = crate::sky::new_water(&planet, &origin);
        }
        let roots = prebuilt.filter(|r| r.planet == planet.id).map(|mut r| std::mem::take(&mut r.chunks));
        commands.remove_resource::<PrebuiltRoots>();
        *terrain = build_terrain(&mut commands, &planet, &mut meshes, material, water, roots);
    }
    let t = terrain.as_mut();
    let mut uploads = 0;
    let mut pending = 0;
    for i in 0..t.nodes.len() {
        if !t.nodes[i].alive || t.nodes[i].task.is_none() {
            continue;
        }
        if uploads >= MAX_UPLOADS_PER_FRAME || !t.nodes[i].task.as_ref().unwrap().is_finished() {
            pending += 1;
            continue;
        }
        let out = block_on(future::poll_once(t.nodes[i].task.take().unwrap())).unwrap();
        let mesh = meshes.add(t.mesh(&out));
        t.nodes[i].mesh = Some(mesh.id());
        let e = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(t.material.clone()),
                Transform::default(),
                Visibility::Hidden,
                WorldPos(planet.centre + DVec3::from_array(out.center)),
                PlanetScene(planet.id),
                TerrainChunk,
            ))
            .id();
        t.spawn_water(&mut commands, &mut meshes, i, e, &out);
        t.nodes[i].entity = Some(e);
        uploads += 1;
    }
    t.pending = pending;
    let cam = origin.view - planet.centre;
    t.visible = 0;
    for r in t.roots.clone() {
        t.update_node(&mut commands, &planet, r, cam);
    }
}
