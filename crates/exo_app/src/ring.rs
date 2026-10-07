//! Terrain collision only near anchors (walker, ship): one heightfield patch per cube-face cell
//! at `patch_depth`, in the cell's tangent frame, heights from planet_core::patch_heights
//! Built on the async compute pool.
use crate::env::{from_v3, to_v3, PlanetRes};
use crate::Layer;
use avian3d::prelude::*;
use bevy::math::{DMat3, DQuat, DVec3};
use bevy::prelude::*;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use planet_core::cube_to_sphere;
use std::collections::HashMap;

pub const PATCH_SAMPLES: usize = 32; // 1 m spacing, 31 m wide
const UPDATE_INTERVAL: f64 = 0.2;
const MAX_ADDS_PER_FRAME: usize = 16;

type Key = (u8, u32, u32);

pub struct PatchOut {
    up: DVec3,
    rot: DQuat,
    heights: Vec<f32>,
}

#[derive(Resource)]
pub struct Ring {
    pub patch_depth: u32,
    pub ring_radius: f64,
    /// World positions to keep covered, with velocity (written by walker/ship systems).
    pub anchors: Vec<(DVec3, DVec3)>,
    pub patches: HashMap<Key, (Entity, DVec3, DQuat)>,
    pending: HashMap<Key, Task<PatchOut>>,
    timer: f64,
}

impl Ring {
    pub fn new(radius: f64) -> Ring {
        let face_edge = radius * std::f64::consts::PI * 0.5;
        Ring {
            patch_depth: (face_edge / 20.0).log2().ceil() as u32,
            ring_radius: 100.0,
            anchors: Vec::new(),
            patches: HashMap::new(),
            pending: HashMap::new(),
            timer: 0.0,
        }
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// True if a built patch covers this world position.
    pub fn has_patch_near(&self, world: DVec3) -> bool {
        let half = PATCH_SAMPLES as f64 * 0.5 - 1.0;
        self.patches.values().any(|(_, c, r)| {
            let l = r.inverse() * (world - *c);
            l.x.abs() < half && l.z.abs() < half
        })
    }

    pub fn force_update(&mut self) {
        self.timer = 0.0;
    }

    fn collect(&self, planet: &PlanetRes, face: usize, a0: f64, b0: f64, size: f64, depth: u32, points: &[DVec3], out: &mut HashMap<Key, (f64, f64, f64, f64)>) {
        let r = planet.radius;
        let centre = from_v3(cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5)) * r;
        // Bound = farthest corner (spike 5 fix).
        let mut bound: f64 = 0.0;
        for (a, b) in [(a0, b0), (a0 + size, b0), (a0, b0 + size), (a0 + size, b0 + size)] {
            bound = bound.max(centre.distance(from_v3(cube_to_sphere(face, a, b)) * r));
        }
        let best = points.iter().map(|p| (p.distance(centre) - bound).max(0.0)).fold(f64::INFINITY, f64::min);
        if best > self.ring_radius * 1.3 {
            return;
        }
        if depth == self.patch_depth {
            let key = (face as u8, ((a0 + 1.0) / size).round() as u32, ((b0 + 1.0) / size).round() as u32);
            out.insert(key, (a0, b0, size, best));
            return;
        }
        let half = size * 0.5;
        for j in 0..2 {
            for i in 0..2 {
                self.collect(planet, face, a0 + i as f64 * half, b0 + j as f64 * half, half, depth + 1, points, out);
            }
        }
    }
}

pub fn update_ring(mut commands: Commands, time: Res<Time>, planet: Res<PlanetRes>, mut ring: ResMut<Ring>) {
    let ring = ring.as_mut();
    let planet = planet.as_ref();
    // Collect finished jobs, a few per frame.
    let mut adds = 0;
    let keys: Vec<Key> = ring.pending.keys().copied().collect();
    for key in keys {
        if adds >= MAX_ADDS_PER_FRAME {
            break;
        }
        if !ring.pending[&key].is_finished() {
            continue;
        }
        let task = ring.pending.remove(&key).unwrap();
        let out = block_on(future::poll_once(task)).expect("finished");
        adds += 1;
        // Avian heightfield: rows along X, columns along Z; planet_core: row-major, z outer.
        let n = PATCH_SAMPLES;
        let heights: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| out.heights[j * n + i] as f64).collect()).collect();
        let centre = planet.centre + out.up * planet.radius;
        let span = (n - 1) as f64;
        let e = commands
            .spawn((
                RigidBody::Static,
                Collider::heightfield(heights, DVec3::new(span, 1.0, span)),
                Position(centre),
                Rotation(out.rot),
                CollisionLayers::new(Layer::World, LayerMask::ALL),
            ))
            .id();
        ring.patches.insert(key, (e, centre, out.rot));
    }

    ring.timer -= time.delta_secs_f64();
    if ring.timer > 0.0 {
        return;
    }
    ring.timer = UPDATE_INTERVAL;
    let mut points = Vec::new();
    for &(p, v) in &ring.anchors {
        for q in [p, p + v * 0.5] {
            // Compare on the base sphere so terrain height does not inflate the ring.
            let rel = q - planet.centre;
            if rel.length() - planet.surface(rel) < ring.ring_radius {
                points.push(rel.normalize() * planet.radius);
            }
        }
    }
    let mut want = HashMap::new();
    for face in 0..6 {
        ring.collect(planet, face, -1.0, -1.0, 2.0, 0, &points, &mut want);
    }
    let pool = AsyncComputeTaskPool::get();
    for (&key, &(a0, b0, size, dist)) in &want {
        if dist < ring.ring_radius && !ring.patches.contains_key(&key) && !ring.pending.contains_key(&key) {
            let pgen = planet.pgen.clone();
            let face = key.0 as usize;
            let task = pool.spawn(async move {
                let mid = size * 0.5;
                let up = from_v3(cube_to_sphere(face, a0 + mid, b0 + mid));
                let east = from_v3(cube_to_sphere(face, a0 + size, b0 + mid)) - from_v3(cube_to_sphere(face, a0, b0 + mid));
                let t = (east - up * east.dot(up)).normalize();
                let b = t.cross(up);
                let heights = pgen.patch_heights(to_v3(up), to_v3(t), to_v3(b), PATCH_SAMPLES);
                PatchOut { up, rot: DQuat::from_mat3(&DMat3::from_cols(t, up, b)), heights }
            });
            ring.pending.insert(key, task);
        }
    }
    let stale: Vec<Key> = ring.patches.keys().filter(|k| !want.contains_key(k)).copied().collect();
    for k in stale {
        let (e, _, _) = ring.patches.remove(&k).unwrap();
        commands.entity(e).despawn();
    }
}
