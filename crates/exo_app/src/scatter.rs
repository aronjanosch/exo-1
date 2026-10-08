//! Scatter on screen (#65): the instances of `planet_core::build_scatter` as props
//! (`content/props/<id>.glb`, from `art/props/props.py`), per cell and storey, built on the async
//! compute pool around the camera. Each storey has its own cell size and range (recipe
//! `scatter.storeys`); trees reach further when the camera is high. View only: no collision.
use crate::env::PlanetRes;
use crate::origin::{RenderOrigin, WorldPos};
use crate::terrain::PlanetScene;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use planet_core::{cube_to_sphere, ScatterCell};
use std::collections::HashMap;

/// Cells built into entities per frame (each is up to a few thousand children).
const MAX_UPLOADS_PER_FRAME: usize = 3;
/// A cell is dropped only beyond this share of its range (no flicker at the edge).
const KEEP_FACTOR: f64 = 1.15;
/// Cells are no smaller than this (m): ground cover would otherwise get tiny cells.
const MIN_CELL_M: f64 = 25.0;
/// Camera height above the ground at which a storey reaches its elevated range (m).
const ELEVATED_AT_M: f64 = 200.0;

type Key = (u8, u8, u32, u32); // storey, face, ix, iy

enum Cell {
    Building(Task<ScatterCell>),
    Shown(Entity),
}

#[derive(Resource, Default)]
pub struct ScatterView {
    for_planet: Option<warp_core::PlanetId>,
    cells: HashMap<Key, Cell>,
    /// Per storey: quadtree depth of its cells and its ranges (m).
    storeys: Vec<(u32, f64, f64)>,
    meshes: HashMap<String, Handle<Mesh>>,
    materials: HashMap<[u8; 3], Handle<StandardMaterial>>,
    /// Entry index -> (plain, rare) prop.
    props: Vec<(String, Option<String>)>,
    pub instances: usize,
    pub shown_cells: usize,
    pub pending: usize,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn edge_at(radius: f64, depth: u32) -> f64 {
    radius * std::f64::consts::FRAC_PI_2 / (1u32 << depth) as f64
}

impl ScatterView {
    fn reset(&mut self, commands: &mut Commands, planet: &PlanetRes) {
        for (_, c) in self.cells.drain() {
            if let Cell::Shown(e) = c {
                commands.entity(e).try_despawn();
            }
        }
        self.for_planet = Some(planet.id);
        self.storeys = planet
            .pgen
            .storeys()
            .iter()
            .map(|(_, s)| {
                let want = (s.range_m / 4.0).max(MIN_CELL_M);
                let mut d = 1;
                while d < 12 && edge_at(planet.radius, d) > want {
                    d += 1;
                }
                (d, s.range_m, s.elevated_range_m.max(s.range_m))
            })
            .collect();
        self.props = planet.pgen.scatter_entries().into_iter().map(|e| (e.prop, e.rare_prop)).collect();
    }

    fn material(&mut self, mats: &mut Assets<StandardMaterial>, tint: [f32; 3]) -> Handle<StandardMaterial> {
        // Quantized, so instances share a few materials and batch.
        let q = tint.map(|c| (c.clamp(0.0, 1.0) * 31.0).round() as u8);
        self.materials
            .entry(q)
            .or_insert_with(|| {
                let c = q.map(|v| srgb_to_linear(v as f32 / 31.0));
                mats.add(StandardMaterial { base_color: Color::linear_rgb(c[0], c[1], c[2]), perceptual_roughness: 0.92, ..default() })
            })
            .clone()
    }

    fn mesh(&mut self, assets: &AssetServer, prop: &str) -> Handle<Mesh> {
        self.meshes
            .entry(prop.to_string())
            .or_insert_with(|| assets.load(GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }.from_asset(format!("props/{prop}.glb"))))
            .clone()
    }
}

/// Visits the quadtree of one storey down to its cell depth, collecting cells within `range`.
fn visit(planet: &PlanetRes, cam: DVec3, face: usize, a0: f64, b0: f64, size: f64, depth: u32, target: u32, range: f64, out: &mut Vec<(usize, f64, f64, f64)>) {
    let r = planet.radius;
    let c = |a: f64, b: f64| {
        let v = cube_to_sphere(face, a, b);
        DVec3::new(v.x, v.y, v.z) * r
    };
    let centre = c(a0 + size * 0.5, b0 + size * 0.5);
    let mut bound: f64 = 0.0;
    for (a, b) in [(a0, b0), (a0 + size, b0), (a0, b0 + size), (a0 + size, b0 + size)] {
        bound = bound.max(centre.distance(c(a, b)));
    }
    if cam.distance(centre) - bound - planet.relief > range {
        return;
    }
    if depth == target {
        out.push((face, a0, b0, size));
        return;
    }
    let h = size * 0.5;
    for j in 0..2 {
        for k in 0..2 {
            visit(planet, cam, face, a0 + k as f64 * h, b0 + j as f64 * h, h, depth + 1, target, range, out);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_scatter(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    origin: Res<RenderOrigin>,
    assets: Res<AssetServer>,
    mut view: ResMut<ScatterView>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let view = view.as_mut();
    if view.for_planet != Some(planet.id) {
        view.reset(&mut commands, &planet);
    }
    let cam = origin.view - planet.centre;
    let agl = planet.above_ground(origin.view).max(0.0);
    let lift = (agl / ELEVATED_AT_M).clamp(0.0, 1.0);
    let mut wanted: HashMap<Key, (usize, f64, f64, f64)> = HashMap::new();
    let mut keep_range = Vec::new();
    for (si, &(depth, near, far)) in view.storeys.iter().enumerate() {
        let range = near + (far - near) * lift;
        keep_range.push(range * KEEP_FACTOR);
        // Above twice its range over the ground a storey is not drawn at all.
        if agl > range * 2.0 {
            continue;
        }
        let mut cells = Vec::new();
        for face in 0..6 {
            visit(&planet, cam, face, -1.0, -1.0, 2.0, 0, depth, range, &mut cells);
        }
        for c in cells {
            let size = c.3;
            let key = (si as u8, c.0 as u8, ((c.1 + 1.0) / size).round() as u32, ((c.2 + 1.0) / size).round() as u32);
            wanted.insert(key, c);
        }
    }
    // Drop cells far outside their range.
    let mut drop = Vec::new();
    for (key, cell) in &view.cells {
        if wanted.contains_key(key) {
            continue;
        }
        let (depth, ..) = view.storeys[key.0 as usize];
        let size = 2.0 / (1u32 << depth) as f64;
        let (a, b) = (-1.0 + (key.2 as f64 + 0.5) * size, -1.0 + (key.3 as f64 + 0.5) * size);
        let v = cube_to_sphere(key.1 as usize, a, b);
        let centre = DVec3::new(v.x, v.y, v.z) * planet.radius;
        if cam.distance(centre) - edge_at(planet.radius, depth) - planet.relief > keep_range[key.0 as usize] || agl > keep_range[key.0 as usize] * 2.0 {
            drop.push(*key);
        }
        let _ = cell;
    }
    for key in drop {
        if let Some(Cell::Shown(e)) = view.cells.remove(&key) {
            commands.entity(e).try_despawn();
        }
    }
    for (key, (face, a0, b0, size)) in wanted {
        view.cells.entry(key).or_insert_with(|| {
            let pgen = planet.pgen.clone();
            let storey = key.0 as usize;
            Cell::Building(AsyncComputeTaskPool::get().spawn(async move { pgen.build_scatter(face, a0, b0, size, storey) }))
        });
    }
    // Upload finished cells.
    let mut uploads = 0;
    let mut pending = 0;
    let keys: Vec<Key> = view.cells.keys().copied().collect();
    for key in keys {
        let Some(Cell::Building(task)) = view.cells.get_mut(&key) else { continue };
        if uploads >= MAX_UPLOADS_PER_FRAME || !task.is_finished() {
            pending += 1;
            continue;
        }
        let out = block_on(future::poll_once(task)).unwrap();
        let parent = commands
            .spawn((Transform::default(), Visibility::default(), WorldPos(planet.centre + DVec3::from_array(out.center)), PlanetScene))
            .id();
        for inst in &out.instances {
            let (plain, rare) = view.props[inst.entry as usize].clone();
            let prop = if inst.rare { rare.unwrap_or(plain) } else { plain };
            let mesh = view.mesh(&assets, &prop);
            let mat = view.material(&mut mats, inst.tint);
            let [x, u, z] = inst.basis.map(Vec3::from_array);
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform { translation: Vec3::from_array(inst.pos), rotation: Quat::from_mat3(&Mat3::from_cols(x, u, z)), scale: Vec3::splat(inst.scale) },
                ChildOf(parent),
            ));
        }
        view.cells.insert(key, Cell::Shown(parent));
        uploads += 1;
    }
    view.pending = pending;
    view.shown_cells = view.cells.values().filter(|c| matches!(c, Cell::Shown(_))).count();
}

/// Instances on screen (F3 line, look harness): counted from the children of the cells.
pub fn count_instances(mut view: ResMut<ScatterView>, cells: Query<&Children, With<PlanetScene>>) {
    let mut n = 0;
    for c in &view.cells {
        if let Cell::Shown(e) = c.1
            && let Ok(ch) = cells.get(*e)
        {
            n += ch.len();
        }
    }
    view.instances = n;
}

pub fn plugin(app: &mut App) {
    app.init_resource::<ScatterView>();
    app.add_systems(Update, (update_scatter, count_instances).chain().after(crate::terrain::update_terrain));
}
