//! Cube-sphere terrain with a quadtree LOD per face (port of terrain.gd). Chunks come from
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
use std::time::Instant;

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
    task: Option<Task<(ChunkOut, f64)>>,
    alive: bool,
}

#[derive(Resource)]
pub struct Terrain {
    nodes: Vec<Node>,
    free: Vec<usize>,
    roots: Vec<usize>,
    max_depth: u32,
    material: Handle<StandardMaterial>,
    indices: Vec<u32>,
    pub visible: usize,
    pub build_ms_sum: f64,
    pub build_count: u32,
    pub build_ms_max: f64,
    pub frame_ms_max: f64,
    pub pending: usize,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

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
            children: Vec::new(), entity: None, task: None, alive: true,
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
        self.nodes[i].task = Some(AsyncComputeTaskPool::get().spawn(async move {
            let t0 = Instant::now();
            let out = pgen.build_chunk(face, a0, b0, size, false);
            (out, t0.elapsed().as_secs_f64() * 1000.0)
        }));
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
        n.task = None; // dropping a bevy Task cancels it
        n.alive = false;
        self.free.push(i);
    }

    fn mesh(&self, out: &ChunkOut) -> Mesh {
        let colors: Vec<[f32; 4]> = out.colors.iter().map(|c| [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), 1.0]).collect();
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, out.verts.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, out.normals.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_indices(Indices::U32(self.indices.clone()))
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let face_edge = planet.radius * std::f64::consts::PI * 0.5;
    let mut indices = Vec::with_capacity((M - 1) * (M - 1) * 6);
    for j in 0..M - 1 {
        for i in 0..M - 1 {
            let k00 = (j * M + i) as u32;
            let (k10, k01) = (k00 + 1, k00 + M as u32);
            let k11 = k01 + 1;
            // Godot winds clockwise for front faces, Bevy counter-clockwise.
            indices.extend([k00, k10, k01, k10, k11, k01]);
        }
    }
    let material = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.95, ..default() });
    let mut t = Terrain {
        nodes: Vec::new(), free: Vec::new(), roots: Vec::new(),
        max_depth: ((face_edge / 37.0).log2().round() as u32).max(1),
        material, indices,
        visible: 0, build_ms_sum: 0.0, build_count: 0, build_ms_max: 0.0, frame_ms_max: 0.0, pending: 0,
    };
    for face in 0..6 {
        let i = t.make_node(&planet, face, -1.0, -1.0, 2.0, 0);
        // Roots synchronously, so there is always a planet.
        let n = &t.nodes[i];
        let out = planet.pgen.build_chunk(n.face, n.a0, n.b0, n.size, false);
        let mesh = meshes.add(t.mesh(&out));
        let e = commands
            .spawn((Mesh3d(mesh), MeshMaterial3d(t.material.clone()), Transform::default(), WorldPos(planet.centre + DVec3::from_array(out.center))))
            .id();
        t.nodes[i].entity = Some(e);
        t.roots.push(i);
    }
    // Water sphere at sea level (no collision, walkable under it, as in spike 8).
    let r = (planet.radius + planet.sea) as f32;
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(r).mesh().uv(128, 64))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.16, 0.38, 0.52, 0.82),
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            double_sided: true,
            perceptual_roughness: 0.35,
            ..default()
        })),
        Transform::default(),
        WorldPos(planet.centre),
    ));
    // Site markers: 24 m orange pillars.
    let pillar = meshes.add(Cylinder::new(0.9, 24.0));
    let orange = materials.add(Color::srgb(1.0, 0.45, 0.1));
    for s in &planet.pgen.sites {
        let dir = from_v3(*s);
        let base = planet.centre + dir * (planet.surface(dir) + 12.0);
        commands.spawn((
            Mesh3d(pillar.clone()),
            MeshMaterial3d(orange.clone()),
            Transform::from_rotation(Quat::from_rotation_arc(Vec3::Y, dir.as_vec3())),
            WorldPos(base),
        ));
    }
    commands.insert_resource(t);
}

pub fn update_terrain(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    origin: Res<RenderOrigin>,
    mut terrain: ResMut<Terrain>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let t0 = Instant::now();
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
        let (out, ms) = block_on(future::poll_once(t.nodes[i].task.take().unwrap())).unwrap();
        t.build_ms_sum += ms;
        t.build_count += 1;
        t.build_ms_max = t.build_ms_max.max(ms);
        let mesh = meshes.add(t.mesh(&out));
        let e = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(t.material.clone()),
                Transform::default(),
                Visibility::Hidden,
                WorldPos(planet.centre + DVec3::from_array(out.center)),
            ))
            .id();
        t.nodes[i].entity = Some(e);
        uploads += 1;
    }
    t.pending = pending;
    let cam = origin.view - planet.centre;
    t.visible = 0;
    for r in t.roots.clone() {
        t.update_node(&mut commands, &planet, r, cam);
    }
    t.frame_ms_max = t.frame_ms_max.max(t0.elapsed().as_secs_f64() * 1000.0);
}
