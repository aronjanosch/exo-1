//! Cube-sphere terrain with a quadtree LOD per face. Chunks (data and finished meshes) come
//! from planet_core::build_chunk on the async compute pool, nearest first, with the camera
//! projected along its motion as prefetch (#215); the main thread only adds the ready meshes
//! and spawns, within a time and byte budget per frame. Merged chunks wait in a small LRU
//! cache, so a merge then split does not rebuild. View only: collision comes from the ring.
use crate::env::{from_v3, PlanetRes};
use crate::origin::{RenderOrigin, WorldPos};
use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy::mesh::{Indices, MeshTag, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use std::collections::HashMap;
use std::sync::Arc;
use planet_core::{cube_to_sphere, ChunkOut, M};

/// One owner of the terrain: the window, and the headless `swap` scenario (which counts what a swap leaves behind).
pub fn plugin(app: &mut App) {
    app.add_systems(Startup, setup_terrain);
    app.add_systems(Update, update_terrain.in_set(crate::phases::Frame::World));
    // Streaming adds and frees meshes all the time: start the GPU slabs large and grow them in
    // big steps, so the allocator rarely regrows and copies a slab mid-flight (#214).
    // `EXO_SLAB_MB=0` keeps Bevy's defaults (1 MiB, x1.5) for comparison.
    let mb: u64 = std::env::var("EXO_SLAB_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(SLAB_MB);
    if mb > 0
        && let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp)
    {
        let mut s = bevy::render::mesh::allocator::MeshAllocatorSettings::default();
        s.slab_allocator_settings.min_slab_size = mb << 20;
        s.slab_allocator_settings.growth_factor = 2.0;
        render.insert_resource(s);
    }
}

/// First size of a GPU mesh slab (MiB). TODO(initiator): value.
const SLAB_MB: u64 = 64;

/// More async threads than Bevy's default 4 for the chunk jobs (#215: the pool was the limit of
/// the streaming at 800 m/s). TODO(initiator): value; the render and main threads need cores too.
pub fn task_pools() -> bevy::app::TaskPoolOptions {
    let mut o = bevy::app::TaskPoolOptions::default();
    o.async_compute.max_threads = 6;
    o.async_compute.percent = 0.4;
    o
}

/// Margin on a node's height range for the detail its children add: metres plus a share of its
/// edge (the parent's grid samples miss the peaks between them).
const BOUND_PAD_M: f64 = 4.0;
const BOUND_PAD_EDGE: f64 = 0.05;
const SPLIT_FACTOR: f64 = 1.5;
const MERGE_FACTOR: f64 = 1.8;
/// Main-thread time per frame for adding ready chunks (at least one chunk is always added).
/// TODO(initiator): value and feel.
const UPLOAD_BUDGET_MS: f64 = 1.5;
/// Mesh bytes handed to the renderer per frame; the render thread's mesh allocator copies them
/// (about 100 kB per chunk with its water; u16 indices, a chunk has 35 x 35 vertices). TODO(initiator): value.
const UPLOAD_BYTES_PER_FRAME: usize = 512 << 10;
/// Chunk jobs in flight per pool thread, so the nearest chunks are not stuck behind far ones.
const JOBS_PER_THREAD: usize = 2;
/// Seconds the camera is projected along its motion for job priority and children prefetch.
/// TODO(initiator): value.
const PREFETCH_S: f64 = 1.5;
/// Speed limit of the projection, so a teleport does not prefetch the far side.
const PREFETCH_MAX_SPEED: f64 = 2000.0;
/// Geomorphing (#220): a chunk is at its own detail while the camera is nearer than
/// `MORPH_NEAR x edge` from its bounding sphere (the distance a chunk merges back into its
/// parent) and in its parent's shape from `MORPH_FAR x edge` (twice the child edge x the split
/// factor: where the parent splits, so a new child appears in the parent's shape).
pub const MORPH_NEAR: f64 = MERGE_FACTOR;
pub const MORPH_FAR: f64 = SPLIT_FACTOR * 2.0;
/// Finished chunks kept after a merge (about 100 kB of GPU memory each).
const CACHE_MAX: usize = 256;

type CacheKey = (usize, u64, u64, u64);

/// A chunk's meshes on the GPU side of the asset system, from the pool or the cache.
#[derive(Clone)]
struct Meshes {
    centre: DVec3,
    /// Lowest and highest ground of the chunk (m above the base radius).
    h: (f64, f64),
    mesh: Handle<Mesh>,
    water: Option<Handle<Mesh>>,
    inland: Option<Handle<Mesh>>,
}

/// What a chunk job returns: the meshes, built on the pool.
pub struct BuiltChunk {
    centre: DVec3,
    h: (f64, f64),
    mesh: Mesh,
    water: Option<Mesh>,
    inland: Option<Mesh>,
    bytes: usize,
}

enum Job {
    /// Nothing to wait for (the chunk is spawned).
    None,
    /// Wanted, not started (waits for a job slot, nearest first).
    Queued,
    Running(Task<BuiltChunk>),
    /// From the cache, spawns without a rebuild.
    Cached(Meshes),
}

struct Node {
    face: usize,
    a0: f64,
    b0: f64,
    size: f64,
    depth: u32,
    centre: DVec3, // planet space, base sphere
    edge_m: f64,
    /// Distance from `centre` to the farthest corner on the base sphere.
    horiz: f64,
    /// Height range the chunk's ground lies in (m above the base radius): the parent's until
    /// the chunk itself is built, then its own (#214: the planet-wide relief made every node
    /// split a kilometre too early at 100 km).
    h: (f64, f64),
    /// Middle of that range above `centre`, and the bounding radius around it.
    mid: DVec3,
    bound: f64,
    children: Vec<usize>,
    entity: Option<Entity>,
    /// Handles of the spawned chunk (and its water, lakes and rivers #72), kept for the cache.
    meshes: Option<Meshes>,
    job: Job,
    /// Whether the entity is set to `Visibility::Inherited` (else Hidden): written on change only.
    shown: bool,
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
    indices: Arc<Vec<u16>>,
    cache: HashMap<CacheKey, (u64, Meshes)>,
    cache_stamp: u64,
    /// Camera in planet space this frame, and projected along its motion.
    cam: DVec3,
    pred: DVec3,
    pub visible: usize,
    pub pending: usize,
    /// Geomorphing on (F4 in the window toggles it; `EXO_NO_MORPH` starts it off).
    pub morph_on: bool,
    /// Seconds of this frame.
    dt: f64,
    /// Chunk jobs started this frame.
    pub started: u32,
    /// The camera's nadir on the cube (face, a, b), set each frame for `lod_debt`.
    nadir: Option<(usize, f64, f64)>,
    /// This frame the chunk under the camera wanted to split but its children were still
    /// building (#213: the shown ground is coarser than wanted).
    pub lod_debt: bool,
    /// Root chunks built on the main thread when this terrain was built (6 without prebuilt roots).
    pub roots_built_here: usize,
}

use crate::terrain_material::{srgb_to_linear, TerrainMaterial, ATTRIBUTE_MORPH_DELTA, ATTRIBUTE_MORPH_NORMAL};

/// The index pattern every chunk shares.
fn chunk_indices() -> Vec<u16> {
    let mut indices = Vec::with_capacity((M - 1) * (M - 1) * 6);
    for j in 0..M - 1 {
        for i in 0..M - 1 {
            let k00 = (j * M + i) as u16;
            let (k10, k01) = (k00 + 1, k00 + M as u16);
            let k11 = k01 + 1;
            // Counter-clockwise front faces (Bevy).
            indices.extend([k00, k10, k01, k10, k11, k01]);
        }
    }
    indices
}

fn water_normals(out: &ChunkOut, w: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let c = DVec3::from_array(out.center);
    w.iter().map(|p| (c + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)).normalize().as_vec3().to_array()).collect()
}

fn water_surface(out: &ChunkOut, w: &[[f32; 3]], tris: &[u32]) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, w.to_vec())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, water_normals(out, w))
        .with_inserted_indices(Indices::U16(tris.iter().map(|&k| k as u16).collect()))
}

/// The meshes of a chunk (pure, runs on the pool): the terrain, its sea, and lakes and rivers
/// (#72, their own vertices and triangles, the sea's material).
pub fn build_meshes(out: &ChunkOut, indices: &[u16]) -> BuiltChunk {
    // The biome palette (#66): ground + cap share as the colour, rock in UV0 + UV1.x, strata
    // share in UV1.y (see terrain_material.rs).
    let colors: Vec<[f32; 4]> = out.colors.iter().map(|c| [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), c[3]]).collect();
    let uv0: Vec<[f32; 2]> = out.rock.iter().map(|c| [srgb_to_linear(c[0]), srgb_to_linear(c[1])]).collect();
    let uv1: Vec<[f32; 2]> = out.rock.iter().map(|c| [srgb_to_linear(c[2]), c[3]]).collect();
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, out.verts.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, out.normals.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv0)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1)
        .with_inserted_attribute(ATTRIBUTE_MORPH_DELTA, out.morph_delta.clone())
        .with_inserted_attribute(ATTRIBUTE_MORPH_NORMAL, VertexAttributeValues::Snorm8x4(out.morph_normals.iter().map(|n| {
            let q = |x: f32| (x * 127.0).round() as i8;
            [q(n[0]), q(n[1]), q(n[2]), 0]
        }).collect()))
        .with_inserted_indices(Indices::U16(indices.to_vec()));
    let nowater = std::env::var("EXO_NO_WATER").is_ok();
    let water = out.water.as_ref().filter(|_| !nowater).map(|w| water_surface(out, w, &out.water_tris));
    let inland = out.inland_water.as_ref().filter(|_| !nowater).map(|(w, tris)| water_surface(out, w, tris));
    let bytes = [Some(&mesh), water.as_ref(), inland.as_ref()].into_iter().flatten().map(|m| m.get_vertex_buffer_size() + m.get_index_buffer_bytes().map_or(0, |b| b.len())).sum();
    BuiltChunk { centre: DVec3::from_array(out.center), h: (out.min_h as f64, out.max_h as f64), mesh, water, inland, bytes }
}

impl Terrain {
    fn make_node(&mut self, planet: &PlanetRes, face: usize, a0: f64, b0: f64, size: f64, depth: u32, h: (f64, f64)) -> usize {
        let r = planet.radius;
        let centre = from_v3(cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5)) * r;
        let edge_m = (from_v3(cube_to_sphere(face, a0, b0)) - from_v3(cube_to_sphere(face, a0 + size, b0))).length() * r;
        let mut bound: f64 = 0.0;
        for (a, b) in [(a0, b0), (a0 + size, b0), (a0, b0 + size), (a0 + size, b0 + size)] {
            bound = bound.max(centre.distance(from_v3(cube_to_sphere(face, a, b)) * r));
        }
        let node = Node {
            face, a0, b0, size, depth, centre, edge_m,
            horiz: bound, h, mid: centre, bound,
            children: Vec::new(), entity: None, meshes: None, job: Job::None, shown: false, alive: true,
        };
        let i = if let Some(i) = self.free.pop() {
            self.nodes[i] = node;
            i
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        };
        self.set_range(i, h);
        i
    }

    /// The node's ground lies between `h.0` and `h.1`: its bounding sphere sits around the middle
    /// of that range, with a margin for the finer detail of the levels below.
    fn set_range(&mut self, i: usize, h: (f64, f64)) {
        let n = &mut self.nodes[i];
        let pad = BOUND_PAD_M + n.edge_m * BOUND_PAD_EDGE;
        let (lo, hi) = (h.0 - pad, h.1 + pad);
        n.h = h;
        n.mid = n.centre * (1.0 + 0.5 * (lo + hi) / n.centre.length());
        n.bound = n.horiz + 0.5 * (hi - lo);
    }

    fn key(n: &Node) -> CacheKey {
        (n.face, n.a0.to_bits(), n.b0.to_bits(), n.size.to_bits())
    }

    fn cache_put(&mut self, key: CacheKey, m: Meshes) {
        self.cache_stamp += 1;
        self.cache.insert(key, (self.cache_stamp, m));
        if self.cache.len() > CACHE_MAX {
            // Dropping the handles frees the GPU memory.
            if let Some(oldest) = self.cache.iter().min_by_key(|(_, (stamp, _))| *stamp).map(|(k, _)| *k) {
                self.cache.remove(&oldest);
            }
        }
    }

    /// A node wants its chunk: from the cache at once, else it waits for a job slot.
    fn want(&mut self, i: usize) {
        let job = match self.cache.remove(&Self::key(&self.nodes[i])) {
            Some((_, m)) => Job::Cached(m),
            None => Job::Queued,
        };
        self.nodes[i].job = job;
    }

    /// Distance to the nearer of the camera and its projection, minus the chunk's bound.
    fn dist(&self, i: usize) -> f64 {
        let n = &self.nodes[i];
        (self.cam.distance(n.mid).min(self.pred.distance(n.mid)) - n.bound).max(0.0)
    }

    /// Starts jobs for the nearest queued chunks while there are free slots.
    fn start_jobs(&mut self, planet: &PlanetRes) {
        let pool = AsyncComputeTaskPool::get();
        let cap = pool.thread_num() * JOBS_PER_THREAD;
        let running = self.nodes.iter().filter(|n| n.alive && matches!(n.job, Job::Running(_))).count();
        if running >= cap {
            return;
        }
        let mut queued: Vec<(f64, usize)> = (0..self.nodes.len()).filter(|&i| self.nodes[i].alive && matches!(self.nodes[i].job, Job::Queued)).map(|i| (self.dist(i), i)).collect();
        queued.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, i) in queued.into_iter().take(cap - running) {
            let n = &self.nodes[i];
            let (face, a0, b0, size) = (n.face, n.a0, n.b0, n.size);
            let pgen = planet.pgen.clone();
            let indices = self.indices.clone();
            self.started += 1;
            self.nodes[i].job = Job::Running(pool.spawn(async move { build_meshes(&pgen.build_chunk(face, a0, b0, size), &indices) }));
        }
    }

    fn discard(&mut self, commands: &mut Commands, i: usize) {
        let children = std::mem::take(&mut self.nodes[i].children);
        for c in children {
            self.discard(commands, c);
        }
        let key = Self::key(&self.nodes[i]);
        let n = &mut self.nodes[i];
        let had_entity = n.entity.take().map(|e| commands.entity(e).despawn()).is_some();
        let kept = n.meshes.take().filter(|_| had_entity);
        // Dropping a bevy Task cancels it.
        let cached = match std::mem::replace(&mut n.job, Job::None) {
            Job::Cached(m) => Some(m),
            _ => kept,
        };
        n.alive = false;
        self.free.push(i);
        if let Some(m) = cached {
            self.cache_put(key, m);
        }
    }

    /// Every mesh this terrain holds (chunks and their water, cached ones too), for the swap check (#14).
    pub fn mesh_ids(&self) -> Vec<AssetId<Mesh>> {
        let live = self.nodes.iter().filter(|n| n.alive).filter_map(|n| n.meshes.as_ref());
        let cached = self.cache.values().map(|(_, m)| m);
        live.chain(cached).flat_map(|m| std::iter::once(m.mesh.id()).chain(m.water.as_ref().map(|h| h.id())).chain(m.inland.as_ref().map(|h| h.id()))).collect()
    }

    /// Adds the meshes of a built chunk to the assets.
    fn add_meshes(meshes: &mut Assets<Mesh>, b: BuiltChunk) -> Meshes {
        Meshes { centre: b.centre, h: b.h, mesh: meshes.add(b.mesh), water: b.water.map(|m| meshes.add(m)), inland: b.inland.map(|m| meshes.add(m)) }
    }

    /// Spawns the chunk entity of node `i`, and its sea, lake and river surfaces as children
    /// (shown and dropped with it).
    fn spawn_chunk(&mut self, commands: &mut Commands, planet: &PlanetRes, i: usize, m: Meshes, visible: bool) -> Entity {
        let mut e = commands.spawn((
            Mesh3d(m.mesh.clone()),
            MeshMaterial3d(self.material.clone()),
            Transform::default(),
            WorldPos(planet.centre + m.centre),
            PlanetScene(planet.id),
            TerrainChunk,
            MeshTag(0),
        ));
        if !visible {
            e.insert(Visibility::Hidden);
        }
        let e = e.id();
        for h in m.water.iter().chain(m.inland.iter()) {
            commands.spawn((Mesh3d(h.clone()), MeshMaterial3d(self.water.clone()), Transform::default(), ChildOf(e)));
        }
        self.set_range(i, m.h);
        let tag = Self::morph_tag(&self.nodes[i], m.centre);
        commands.entity(e).insert(MeshTag(tag));
        let n = &mut self.nodes[i];
        n.entity = Some(e);
        n.meshes = Some(m);
        n.shown = visible;
        e
    }

    /// The chunk's geomorph inputs for the vertex shader (#220), fixed for its life: its edge and
    /// its bounding radius around the chunk's origin, each as log2(m) x 4096 in 16 bits. The shader
    /// takes the morph factor from the camera distance, so no tag changes per frame (#214: every
    /// tag change re-queued the chunk, 6-10 ms in the render queue at speed). Roots: 0, no morph.
    fn morph_tag(n: &Node, origin: DVec3) -> u32 {
        if n.depth == 0 {
            return 0;
        }
        // The quadtree measures from the middle of the height range; the shader from the origin.
        let bound = n.bound + n.mid.distance(origin);
        let q = |m: f64| (m.max(1.0).log2() * 4096.0).round().clamp(1.0, 65535.0) as u32;
        q(n.edge_m) | (q(bound) << 16)
    }

    fn set_shown(&mut self, commands: &mut Commands, i: usize, shown: bool) {
        let n = &mut self.nodes[i];
        if n.shown == shown {
            return;
        }
        if let Some(e) = n.entity {
            commands.entity(e).insert(if shown { Visibility::Inherited } else { Visibility::Hidden });
            n.shown = shown;
        }
    }

    /// Hides the children (kept for the prefetch) of a node that is on screen itself.
    fn hide_below(&mut self, commands: &mut Commands, i: usize) {
        for k in 0..self.nodes[i].children.len() {
            let c = self.nodes[i].children[k];
            self.set_shown(commands, c, false);
            self.hide_below(commands, c);
        }
    }

    /// Returns true when this node (or its children) is on screen.
    fn update_node(&mut self, commands: &mut Commands, planet: &PlanetRes, i: usize) {
        let n = &self.nodes[i];
        let d_real = (self.cam.distance(n.mid) - n.bound).max(0.0);
        let d_pred = (self.pred.distance(n.mid) - n.bound).max(0.0);
        let limit = n.edge_m * if n.children.is_empty() { SPLIT_FACTOR } else { MERGE_FACTOR };
        let can_split = n.depth < self.max_depth;
        let want_split = can_split && d_real < limit;
        // The children are built (and kept) for the projected camera too, but shown for the real one.
        let want_kids = can_split && d_real.min(d_pred) < limit;
        if want_kids && self.nodes[i].children.is_empty() {
            let n = &self.nodes[i];
            let (face, a0, b0, half, depth, h) = (n.face, n.a0, n.b0, n.size * 0.5, n.depth + 1, n.h);
            let mut kids = Vec::with_capacity(4);
            for j in 0..2 {
                for k in 0..2 {
                    let c = self.make_node(planet, face, a0 + k as f64 * half, b0 + j as f64 * half, half, depth, h);
                    self.want(c);
                    kids.push(c);
                }
            }
            self.nodes[i].children = kids;
        } else if !want_kids && !self.nodes[i].children.is_empty() {
            for c in std::mem::take(&mut self.nodes[i].children) {
                self.discard(commands, c);
            }
        }
        if want_split {
            let kids = self.nodes[i].children.clone();
            if kids.iter().all(|&c| self.nodes[c].entity.is_some()) {
                self.set_shown(commands, i, false);
                for c in kids {
                    self.update_node(commands, planet, c);
                }
                return;
            }
            // Children still building: keep the parent on screen, no holes.
            let n = &self.nodes[i];
            if self.nadir.is_some_and(|(f, a, b)| f == n.face && a >= n.a0 && a <= n.a0 + n.size && b >= n.b0 && b <= n.b0 + n.size) {
                self.lod_debt = true;
            }
        }
        self.set_shown(commands, i, true);
        self.hide_below(commands, i);
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
    let mut t = Terrain {
        for_planet: planet.id,
        nodes: Vec::new(), free: Vec::new(), roots: Vec::new(),
        max_depth: ((face_edge / 37.0).log2().round() as u32).max(1),
        material, water, indices: Arc::new(chunk_indices()), cache: HashMap::new(), cache_stamp: 0,
        cam: DVec3::ZERO, pred: DVec3::ZERO,
        visible: 0, pending: 0, morph_on: crate::terrain_material::morph_default(), dt: 0.0, started: 0, nadir: None, lod_debt: false, roots_built_here: 0,
    };
    // Roots synchronously (or prebuilt on the pool, #34), so there is always a planet.
    let roots = prebuilt.filter(|r| r.len() == 6).unwrap_or_else(|| {
        t.roots_built_here = 6;
        build_roots(planet)
    });
    for (face, out) in roots.into_iter().enumerate() {
        let i = t.make_node(planet, face, ROOT.0, ROOT.1, ROOT.2, 0, (-planet.relief, planet.relief));
        let m = Terrain::add_meshes(meshes, build_meshes(&out, &t.indices));
        t.spawn_chunk(commands, planet, i, m, true);
        t.roots.push(i);
    }
    t
}

#[allow(clippy::too_many_arguments)]
pub fn update_terrain(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    origin: Res<RenderOrigin>,
    time: Res<Time<Real>>,
    mut terrain: ResMut<Terrain>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain_mats: ResMut<Assets<TerrainMaterial>>,
    mut waters: ResMut<Assets<crate::sky::WaterMaterial>>,
    scene: Query<Entity, With<PlanetScene>>,
    prebuilt: Option<ResMut<PrebuiltRoots>>,
    mut cost: ResMut<crate::frame_cost::FrameCost>,
    mut motion: Local<Option<(DVec3, DVec3)>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
) {
    let t_sys = std::time::Instant::now();
    if terrain.for_planet != planet.id {
        // The simulation's planet changed (warp or teleport): free the old terrain, build the new.
        for e in &scene {
            commands.entity(e).despawn();
        }
        let material = terrain.material.clone();
        let morph_on = terrain.morph_on;
        if let Some(mut m) = terrain_mats.get_mut(&material) {
            *m = crate::terrain_material::new_material(&planet, &origin);
            m.extension.morph.on.x = f32::from(morph_on);
        }
        let water = terrain.water.clone();
        if let Some(mut m) = waters.get_mut(&water) {
            *m = crate::sky::new_water(&planet, &origin);
        }
        let roots = prebuilt.filter(|r| r.planet == planet.id).map(|mut r| std::mem::take(&mut r.chunks));
        commands.remove_resource::<PrebuiltRoots>();
        *terrain = build_terrain(&mut commands, &planet, &mut meshes, material, water, roots);
        terrain.morph_on = morph_on;
        *motion = None;
    }
    let t = terrain.as_mut();
    t.dt = time.delta_secs_f64();
    // Dev toggle (#220): F4 flips the geomorphing, to compare by feel.
    if keys.is_some_and(|k| k.just_pressed(KeyCode::F4)) {
        t.morph_on = !t.morph_on;
        let on = f32::from(t.morph_on);
        if let Some(mut m) = terrain_mats.get_mut(&t.material) {
            m.extension.morph.on.x = on;
        }
    }
    // The camera and where it will be in a moment (smoothed velocity), for priority and prefetch.
    let cam = origin.view - planet.centre;
    let dt = time.delta_secs_f64();
    let vel = match *motion {
        Some((prev, v)) if dt > 1e-4 => v.lerp((cam - prev) / dt, 0.3),
        Some((_, v)) => v,
        None => DVec3::ZERO,
    };
    let vel = if vel.length() > PREFETCH_MAX_SPEED { vel.normalize() * PREFETCH_MAX_SPEED } else { vel };
    *motion = Some((cam, vel));
    t.cam = cam;
    t.pred = cam + vel * PREFETCH_S;

    // Add finished chunks (nearest first) within the time and byte budget.
    let t_up = std::time::Instant::now();
    let mut ready: Vec<(f64, usize)> = (0..t.nodes.len())
        .filter(|&i| t.nodes[i].alive && match &t.nodes[i].job {
            Job::Cached(_) => true,
            Job::Running(task) => task.is_finished(),
            _ => false,
        })
        .map(|i| (t.dist(i), i))
        .collect();
    ready.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut uploads = 0;
    let mut bytes = 0;
    for (_, i) in ready {
        if uploads > 0 && (bytes >= UPLOAD_BYTES_PER_FRAME || t_up.elapsed().as_secs_f64() * 1000.0 >= UPLOAD_BUDGET_MS) {
            break;
        }
        let m = match std::mem::replace(&mut t.nodes[i].job, Job::None) {
            Job::Cached(m) => m,
            Job::Running(task) => {
                let built = block_on(future::poll_once(task)).unwrap();
                bytes += built.bytes;
                Terrain::add_meshes(&mut meshes, built)
            }
            _ => continue,
        };
        t.spawn_chunk(&mut commands, &planet, i, m, false);
        uploads += 1;
    }
    cost.terrain_up.add(uploads as u32, t_up);
    t.visible = 0;
    t.started = 0;
    t.lod_debt = false;
    t.nadir = cam.try_normalize().map(|d| {
        let face = planet_core::face_of(crate::env::to_v3(d));
        let (a, b) = planet_core::sphere_to_face_ab(face, crate::env::to_v3(d));
        (face, a, b)
    });
    for r in t.roots.clone() {
        t.update_node(&mut commands, &planet, r);
    }
    t.start_jobs(&planet);
    t.pending = t.nodes.iter().filter(|n| n.alive && !matches!(n.job, Job::None)).count();
    cost.terrain_sys.add(t.started, t_sys);
    cost.lod_debt = t.lod_debt;
    cost.terrain_pending = t.pending as u32;
}
