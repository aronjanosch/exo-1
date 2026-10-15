//! Merging beats GPU instancing here: it needs no custom pipeline, culls per cell, and the cost
//! moves off the main thread. The prop vertex data is read once from the loaded GLB meshes. The
//! cell walk runs only after the camera moved a few metres, at most `MAX_TASKS` cells build at
//! once, nearest first.
use crate::env::PlanetRes;
use crate::origin::{RenderOrigin, WorldPos};
use crate::terrain::PlanetScene;
use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::VisibilityRange;
use bevy::math::DVec3;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use planet_core::{cube_to_sphere, Instance};
use std::collections::HashMap;
use std::sync::Arc;

const MAX_UPLOADS_PER_FRAME: usize = 8;
const MAX_TASKS: usize = 8;
/// A cell fades in (dithered, by the camera's distance to its centre) over the outer share of its
/// storey's range, so a cell that appears at the edge does not pop. TODO(initiator): value.
const FADE_FROM: f32 = 0.9;
const WALK_MOVE_M: f64 = 6.0;
/// A cell is dropped only beyond this share of its range (no flicker at the edge).
const KEEP_FACTOR: f64 = 1.15;
/// Cells are no smaller than this (m): ground cover would otherwise get tiny cells.
const MIN_CELL_M: f64 = 25.0;
const ELEVATED_AT_M: f64 = 200.0;

type Key = (u8, u8, u32, u32); // storey, face, ix, iy
type CellSpec = (usize, f64, f64, f64); // face, a0, b0, size

pub struct PropGeom {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl PropGeom {
    fn from_mesh(mesh: &Mesh) -> Option<PropGeom> {
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
        let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
            _ => vec![[0.0, 1.0, 0.0]; pos.len()],
        };
        let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(c)) => c.clone(),
            _ => vec![[1.0; 4]; pos.len()],
        };
        let indices = match mesh.indices()? {
            Indices::U16(i) => i.iter().map(|&v| v as u32).collect(),
            Indices::U32(i) => i.clone(),
        };
        Some(PropGeom { positions: pos.clone(), normals, colors, indices })
    }

    /// Stand-in where no glTF loader runs (headless scenarios): about the size of a tree prop.
    fn placeholder() -> PropGeom {
        let m: Mesh = Sphere::new(1.0).mesh().ico(3).unwrap();
        PropGeom::from_mesh(&m).expect("ico sphere")
    }
}

struct CellOut {
    center: [f64; 3],
    instances: usize,
    /// The cell's merged mesh (none when empty), with its bounds.
    parts: Vec<(Mesh, Aabb)>,
}

enum Cell {
    Building(Task<CellOut>),
    Shown { entities: Vec<Entity>, instances: usize },
}

#[derive(Resource, Default)]
pub struct ScatterView {
    for_planet: Option<warp_core::PlanetId>,
    cells: HashMap<Key, Cell>,
    /// Wanted cells without a task yet, nearest first (rebuilt by each walk).
    todo: Vec<(Key, CellSpec)>,
    /// Camera position (planet space) and height of the last walk.
    walked: Option<(DVec3, f64)>,
    /// Per storey: quadtree depth of its cells and its ranges (m).
    storeys: Vec<(u32, f64, f64)>,
    /// Per storey: the camera's distance to a cell at the edge of its range at the last walk (m;
    /// the range lifted with the camera's height, and that height counted in).
    range_now: Vec<f64>,
    sources: HashMap<String, Handle<Mesh>>,
    geoms: Option<Arc<HashMap<String, PropGeom>>>,
    material: Option<Handle<StandardMaterial>>,
    /// Entry index -> (plain, rare) prop.
    props: Arc<Vec<(String, Option<String>)>>,
    placeholder: bool,
    pub instances: usize,
    pub shown_cells: usize,
    pub entities: usize,
    pub pending: usize,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn edge_at(radius: f64, depth: u32) -> f64 {
    radius * std::f64::consts::FRAC_PI_2 / (1u32 << depth) as f64
}

fn cell_centre(radius: f64, face: usize, a0: f64, b0: f64, size: f64) -> DVec3 {
    let v = cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5);
    DVec3::new(v.x, v.y, v.z) * radius
}

impl ScatterView {
    fn reset(&mut self, commands: &mut Commands, planet: &PlanetRes, assets: &AssetServer) {
        for (_, c) in self.cells.drain() {
            if let Cell::Shown { entities, .. } = c {
                for e in entities {
                    commands.entity(e).try_despawn();
                }
            }
        }
        self.todo.clear();
        self.walked = None;
        self.geoms = None;
        self.sources.clear();
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
        let props: Vec<(String, Option<String>)> = planet.pgen.scatter_entries().into_iter().map(|e| (e.prop, e.rare_prop)).collect();
        if !self.placeholder {
            for id in props.iter().flat_map(|(p, r)| std::iter::once(p).chain(r.iter())) {
                self.sources
                    .entry(id.clone())
                    .or_insert_with(|| assets.load(GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }.from_asset(format!("props/{id}.glb"))));
            }
        }
        self.props = Arc::new(props);
    }

    /// Reads the vertex data of every prop once all their meshes are loaded.
    fn collect_geoms(&mut self, meshes: &Assets<Mesh>, assets: &AssetServer) {
        if self.geoms.is_some() {
            return;
        }
        let mut out = HashMap::new();
        if self.placeholder {
            for id in self.props.iter().flat_map(|(p, r)| std::iter::once(p).chain(r.iter())) {
                out.entry(id.clone()).or_insert_with(PropGeom::placeholder);
            }
        } else {
            for (id, h) in &self.sources {
                let Some(mesh) = meshes.get(h) else {
                    if assets.load_state(h).is_failed() {
                        warn!("scatter: prop {id} did not load");
                        continue;
                    }
                    return;
                };
                let Some(g) = PropGeom::from_mesh(mesh) else { return };
                out.insert(id.clone(), g);
            }
        }
        self.geoms = Some(Arc::new(out));
        self.sources.clear();
    }
}

/// Merges the instances of a cell into one mesh: every prop shares the white vertex-colour
/// material, so one entity per cell is enough (every mesh entity is reprocessed by the
/// renderer on an origin shift, so fewer entities mean shorter shift frames).
fn build_cell(geoms: &HashMap<String, PropGeom>, props: &[(String, Option<String>)], center: [f64; 3], instances: &[Instance]) -> CellOut {
    let geom = |inst: &Instance| {
        let (plain, rare) = &props[inst.entry as usize];
        let prop = if inst.rare { rare.as_ref().unwrap_or(plain) } else { plain };
        geoms.get(prop.as_str())
    };
    let (nv, ni) = instances.iter().filter_map(|i| geom(i)).fold((0, 0), |(v, i), g| (v + g.positions.len(), i + g.indices.len()));
    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(nv);
    let mut nrm: Vec<[f32; 3]> = Vec::with_capacity(nv);
    let mut col: Vec<[f32; 4]> = Vec::with_capacity(nv);
    let mut idx: Vec<u32> = Vec::with_capacity(ni);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for inst in instances {
        let Some(g) = geom(inst) else { continue };
        let base = pos.len() as u32;
        let [x, u, z] = inst.basis.map(Vec3::from_array);
        let at = Vec3::from_array(inst.pos);
        let tint = inst.tint.map(srgb_to_linear);
        for ((p, nm), c) in g.positions.iter().zip(&g.normals).zip(&g.colors) {
            let p = Vec3::from_array(*p) * inst.scale;
            let w = at + x * p.x + u * p.y + z * p.z;
            lo = lo.min(w);
            hi = hi.max(w);
            pos.push(w.to_array());
            let n = Vec3::from_array(*nm);
            nrm.push((x * n.x + u * n.y + z * n.z).to_array());
            col.push([c[0] * tint[0], c[1] * tint[1], c[2] * tint[2], c[3]]);
        }
        idx.extend(g.indices.iter().map(|i| i + base));
    }
    let mut parts = Vec::with_capacity(1);
    let mut center = center;
    if !pos.is_empty() {
        // The mesh's origin in the middle of its bounds, on the ground: the cell's centre is on
        // the base sphere, and the distance fade measures from the origin on the GPU.
        let mid = (lo + hi) * 0.5;
        for p in &mut pos {
            *p = (Vec3::from_array(*p) - mid).to_array();
        }
        (lo, hi) = (lo - mid, hi - mid);
        center = [center[0] + mid.x as f64, center[1] + mid.y as f64, center[2] + mid.z as f64];
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_indices(Indices::U32(idx));
        parts.push((mesh, Aabb::from_min_max(lo, hi)));
    }
    CellOut { center, instances: instances.len(), parts }
}

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
    // `cam` is the point under the camera on the base sphere: the height is the caller's
    // (a planet-wide relief here would make every storey walk and keep cells a kilometre out).
    if cam.distance(centre) - bound > range {
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut cost: ResMut<crate::frame_cost::FrameCost>,
) {
    let t_sys = std::time::Instant::now();
    let view = view.as_mut();
    if view.for_planet != Some(planet.id) {
        view.reset(&mut commands, &planet, &assets);
    }
    view.collect_geoms(&meshes, &assets);
    let cam = origin.view - planet.centre;
    let agl = planet.above_ground(origin.view).max(0.0);
    let walk = view.walked.is_none_or(|(p, h)| p.distance(cam) > WALK_MOVE_M || (h - agl).abs() > WALK_MOVE_M);
    if walk {
        view.walked = Some((cam, agl));
        walk_cells(&mut commands, view, &planet, cam, agl);
    }
    if let Some(geoms) = view.geoms.clone() {
        let mut building = view.cells.values().filter(|c| matches!(c, Cell::Building(_))).count();
        let mut started = 0;
        while building < MAX_TASKS && started < view.todo.len() {
            let (key, (face, a0, b0, size)) = view.todo[started];
            started += 1;
            let pgen = planet.pgen.clone();
            let props = view.props.clone();
            let geoms = geoms.clone();
            let storey = key.0 as usize;
            let task = AsyncComputeTaskPool::get().spawn(async move {
                let out = pgen.build_scatter(face, a0, b0, size, storey);
                build_cell(&geoms, &props, out.center, &out.instances)
            });
            view.cells.insert(key, Cell::Building(task));
            building += 1;
        }
        view.todo.drain(..started);
    }
    let t_up = std::time::Instant::now();
    let mut uploads = 0;
    let mut building = 0;
    let keys: Vec<Key> = view.cells.iter().filter(|(_, c)| matches!(c, Cell::Building(_))).map(|(k, _)| *k).collect();
    for key in keys {
        let Some(Cell::Building(task)) = view.cells.get_mut(&key) else { continue };
        if uploads >= MAX_UPLOADS_PER_FRAME || !task.is_finished() {
            building += 1;
            continue;
        }
        let out = block_on(future::poll_once(task)).unwrap();
        let mat = view
            .material
            .get_or_insert_with(|| mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.92, ..default() }))
            .clone();
        // The range it was asked for, measured to the mesh's origin (the middle of its bounds).
        let range = view.range_now.get(key.0 as usize).copied().unwrap_or(f64::MAX) as f32;
        let fade = VisibilityRange { start_margin: 0.0..0.0, end_margin: range * FADE_FROM..range, use_aabb: false };
        let mut entities = Vec::with_capacity(out.parts.len());
        for (mesh, aabb) in out.parts {
            entities.push(
                commands
                    .spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(mat.clone()),
                        aabb,
                        fade.clone(),
                        Transform::default(),
                        WorldPos(planet.centre + DVec3::from_array(out.center)),
                        PlanetScene(planet.id),
                    ))
                    .id(),
            );
        }
        cost.scatter_inst += out.instances as u32;
        cost.scatter_ents += entities.len() as u32;
        view.cells.insert(key, Cell::Shown { entities, instances: out.instances });
        uploads += 1;
    }
    cost.scatter_up.add(uploads as u32, t_up);
    view.pending = building + view.todo.len();
    cost.scatter_sys.add(0, t_sys);
}

fn walk_cells(commands: &mut Commands, view: &mut ScatterView, planet: &PlanetRes, cam: DVec3, agl: f64) {
    // Distances on the base sphere, from the point under the camera; its height over the ground
    // is `agl`, which lifts the ranges and switches storeys off.
    let cam = cam.normalize_or_zero() * planet.radius;
    let lift = (agl / ELEVATED_AT_M).clamp(0.0, 1.0);
    let mut wanted: HashMap<Key, CellSpec> = HashMap::new();
    let mut keep_range = Vec::new();
    view.range_now.clear();
    for (si, &(depth, near, far)) in view.storeys.iter().enumerate() {
        let range = near + (far - near) * lift;
        keep_range.push(range * KEEP_FACTOR);
        view.range_now.push(range.hypot(agl));
        if agl > range * 2.0 {
            continue;
        }
        let mut cells = Vec::new();
        for face in 0..6 {
            visit(planet, cam, face, -1.0, -1.0, 2.0, 0, depth, range, &mut cells);
        }
        for c in cells {
            let size = c.3;
            let key = (si as u8, c.0 as u8, ((c.1 + 1.0) / size).round() as u32, ((c.2 + 1.0) / size).round() as u32);
            wanted.insert(key, c);
        }
    }
    let mut drop = Vec::new();
    for key in view.cells.keys() {
        if wanted.contains_key(key) {
            continue;
        }
        let (depth, ..) = view.storeys[key.0 as usize];
        let size = 2.0 / (1u32 << depth) as f64;
        let (a, b) = (-1.0 + (key.2 as f64 + 0.5) * size, -1.0 + (key.3 as f64 + 0.5) * size);
        let v = cube_to_sphere(key.1 as usize, a, b);
        let centre = DVec3::new(v.x, v.y, v.z) * planet.radius;
        if cam.distance(centre) - edge_at(planet.radius, depth) > keep_range[key.0 as usize] || agl > keep_range[key.0 as usize] * 2.0 {
            drop.push(*key);
        }
    }
    for key in drop {
        if let Some(Cell::Shown { entities, .. }) = view.cells.remove(&key) {
            for e in entities {
                commands.entity(e).try_despawn();
            }
        }
    }
    let mut todo: Vec<(f64, Key, CellSpec)> = wanted
        .into_iter()
        .filter(|(k, _)| !view.cells.contains_key(k))
        .map(|(k, c)| (cam.distance(cell_centre(planet.radius, c.0, c.1, c.2, c.3)), k, c))
        .collect();
    todo.sort_by(|a, b| a.0.total_cmp(&b.0));
    view.todo = todo.into_iter().map(|(_, k, c)| (k, c)).collect();
}

pub fn count_instances(mut view: ResMut<ScatterView>) {
    let (mut n, mut cells, mut ents) = (0, 0, 0);
    for c in view.cells.values() {
        if let Cell::Shown { entities, instances } = c {
            n += instances;
            cells += 1;
            ents += entities.len();
        }
    }
    view.instances = n;
    view.shown_cells = cells;
    view.entities = ents;
}

pub fn plugin(app: &mut App) {
    app.insert_resource(ScatterView { placeholder: !app.is_plugin_added::<bevy::gltf::GltfPlugin>(), ..default() });
    app.add_systems(Update, (update_scatter, count_instances).chain().after(crate::terrain::update_terrain).in_set(crate::phases::Frame::World));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(entry: u16, rare: bool, x: f32) -> Instance {
        Instance { entry, rare, pos: [x, 0.0, 0.0], basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], scale: 2.0, tint: [1.0, 0.5, 0.0] }
    }

    #[test]
    fn a_cell_merges_into_one_mesh() {
        let g = || PropGeom::placeholder();
        let geoms: HashMap<String, PropGeom> = [("tree".to_string(), g()), ("big_tree".to_string(), g())].into();
        let props = vec![("tree".to_string(), Some("big_tree".to_string()))];
        let n = geoms["tree"].positions.len();
        let out = build_cell(&geoms, &props, [0.0; 3], &[inst(0, false, 0.0), inst(0, false, 10.0), inst(0, true, 20.0)]);
        assert_eq!(out.instances, 3);
        assert_eq!(out.parts.len(), 1, "plain and rare prop in one mesh");
        let mesh = &out.parts[0].0;
        assert_eq!(mesh.count_vertices(), 3 * n);
        // Instances at x = 0, 10, 20, scaled by 2 (placeholder radius 1): x spans -2..22. The mesh's
        // origin moves to the middle of that, x = 10 from the cell's centre.
        let a = out.parts[0].1;
        assert!(a.center.x.abs() < 0.1 && (a.half_extents.x - 12.0).abs() < 0.1, "{a:?}");
        assert!((out.center[0] - 10.0).abs() < 0.1, "origin at {:?}", out.center);
        // Tint: linear of (1, .5, 0) on a white vertex colour.
        let Some(VertexAttributeValues::Float32x4(c)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!() };
        assert!((c[0][0] - 1.0).abs() < 1e-5 && c[0][1] < 0.3 && c[0][2] == 0.0);
        assert!(build_cell(&geoms, &props, [0.0; 3], &[]).parts.is_empty());
    }
}
