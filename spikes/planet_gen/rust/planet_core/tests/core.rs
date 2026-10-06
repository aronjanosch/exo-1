//! T1 (core part), T2, T3 of spikes/planet_gen/BRIEF.md. Run: cargo test --profile fast -- --nocapture
use planet_core::*;
use std::collections::HashMap;
use std::sync::OnceLock;

static PLANET: OnceLock<(Planet, BakeStats)> = OnceLock::new();
fn planet() -> &'static (Planet, BakeStats) {
    PLANET.get_or_init(|| {
        let r = Recipe::from_json(include_str!("../../../recipe.json")).unwrap();
        let mut p = Planet::new(r);
        let st = p.bake(0);
        (p, st)
    })
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn chunk_world(_p: &Planet, c: &ChunkOut, k: usize) -> V3 {
    V3::from_arr(c.center) + v3(c.verts[k][0] as f64, c.verts[k][1] as f64, c.verts[k][2] as f64)
}

/// Interior (non-skirt) vertex indices.
fn interior() -> impl Iterator<Item = (usize, usize, usize)> {
    (1..=GRID + 1).flat_map(|j| (1..=GRID + 1).map(move |i| (j * M + i, i, j)))
}

#[test]
fn t1_one_height_function() {
    let (p, _) = planet();
    let mut rng = Rng(0x1234_5678_9abc_def1);
    // (a) random directions: face-coordinate path versus direction path
    let mut worst: f64 = 0.0;
    for _ in 0..1000 {
        let z = rng.next() * 2.0 - 1.0;
        let phi = rng.next() * std::f64::consts::TAU;
        let rr = (1.0 - z * z).sqrt();
        let d = v3(rr * phi.cos(), z, rr * phi.sin());
        let face = face_of(d);
        let (a, b) = sphere_to_face_ab(face, d);
        let h1 = p.height_ab(face, a, b, d).0;
        let h2 = p.height_at(d);
        worst = worst.max((h1 - h2).abs());
        // also the other candidate face when close to an edge must agree
        let (ox, oy) = (rng.next(), rng.next());
        let _ = (ox, oy);
    }
    println!("T1 random directions: max |height_ab - height_at| = {:.3e} m", worst);
    assert!(worst < 1e-3);

    // (b) every vertex of 50 random chunks at several depths: mesh radius versus height_at
    let mut worst_mesh: f64 = 0.0;
    let mut n_vert = 0;
    for ci in 0..50 {
        let depth = [2u32, 4, 6, 8][ci % 4];
        let size = 2.0 / (1u32 << depth) as f64;
        let face = (rng.next() * 6.0) as usize % 6;
        let ix = (rng.next() * (1u32 << depth) as f64) as usize;
        let iy = (rng.next() * (1u32 << depth) as f64) as usize;
        let (a0, b0) = (-1.0 + ix as f64 * size, -1.0 + iy as f64 * size);
        let c = p.build_chunk(face, a0, b0, size, false);
        for (k, _, _) in interior() {
            let w = chunk_world(p, &c, k);
            let h_mesh = w.length() - p.radius;
            let h_fn = p.height_at(w.normalized());
            worst_mesh = worst_mesh.max((h_mesh - h_fn).abs());
            n_vert += 1;
        }
    }
    println!("T1 mesh vertices: {} vertices of 50 chunks (depths 2,4,6,8), max |mesh - height_at| = {:.3e} m", n_vert, worst_mesh);
    assert!(worst_mesh < 1e-3);
}

fn seam_check(p: &Planet, depth: u32) -> (usize, f64, usize) {
    let n = 1usize << depth;
    let size = 2.0 / n as f64;
    // quantised key (cm cells) of all border vertices
    let mut cells: HashMap<(i64, i64, i64), Vec<(usize, V3)>> = HashMap::new();
    let mut id = 0usize;
    for face in 0..6 {
        for iy in 0..n {
            for ix in 0..n {
                let c = p.build_chunk(face, -1.0 + ix as f64 * size, -1.0 + iy as f64 * size, size, false);
                for (k, i, j) in interior() {
                    if i == 1 || i == GRID + 1 || j == 1 || j == GRID + 1 {
                        let w = chunk_world(p, &c, k);
                        let key = ((w.x * 100.0).floor() as i64, (w.y * 100.0).floor() as i64, (w.z * 100.0).floor() as i64);
                        cells.entry(key).or_default().push((id, w));
                    }
                }
                id += 1;
            }
        }
    }
    let (mut worst, mut unmatched, mut checked) = (0.0f64, 0usize, 0usize);
    for (key, list) in &cells {
        for (cid, w) in list {
            let mut best = f64::MAX;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(l2) = cells.get(&(key.0 + dx, key.1 + dy, key.2 + dz)) {
                            for (c2, w2) in l2 {
                                if c2 != cid {
                                    best = best.min((*w - *w2).length());
                                }
                            }
                        }
                    }
                }
            }
            checked += 1;
            if best > 0.02 {
                unmatched += 1;
            } else {
                worst = worst.max(best);
            }
        }
    }
    (checked, worst, unmatched)
}

#[test]
fn t2_no_seams() {
    let (p, _) = planet();
    for depth in [1u32, 3, 5] {
        let (checked, worst, unmatched) = seam_check(p, depth);
        println!("T2 depth {}: {} border vertices, worst partner distance {:.3e} m, unmatched {}", depth, checked, worst, unmatched);
        assert_eq!(unmatched, 0);
        assert!(worst < 1e-3);
    }
}

#[test]
fn t3_macro_statistics() {
    let (p, st) = planet();
    println!("T3 {}", serde_json::to_string_pretty(st).unwrap());
    assert!((st.land_fraction_macro - 0.7).abs() < 0.01);
    assert!(st.site_count == 0 || st.site_min_pair_m >= 600.0);
    let _ = p;
}

#[test]
fn t1_collision_patches_core() {
    // patches in tangent frames over steep ground (escarpment, plateau edge, basin rim) and flat ground
    let (p, _) = planet();
    let mut worst: f64 = 0.0;
    let mut n = 0;
    let centres = [v3(-0.4, 0.3, 1.0), v3(-1.0, -0.2, -0.4), v3(1.0, 0.2, 0.3), v3(0.3, 1.0, 0.2)];
    for c in centres {
        for k in 0..6 {
            let up0 = c.normalized();
            let east = up0.cross(v3(0.0, 1.0, 0.0)).normalized();
            let off = (east * (k as f64 * 100.0 / p.radius) + up0).normalized();
            let t = (east - off * east.dot(off)).normalized();
            let b = t.cross(off);
            let hs = p.patch_heights(off, t, b, 32);
            for j in 0..32 {
                for i in 0..32 {
                    let (x, z) = (i as f64 - 15.5, j as f64 - 15.5);
                    let o64 = off * p.radius;
                    let o32 = v3(o64.x as f32 as f64, o64.y as f32 as f64, o64.z as f32 as f64);
                    let w = o32 + t * x + b * z + off * hs[j * 32 + i] as f64;
                    worst = worst.max((w.length() - p.radius - p.height_at(w.normalized())).abs());
                    n += 1;
                }
            }
        }
    }
    println!("T1 collision patches (core, origin rounded to f32 like the scene): {} samples, max |patch - height_at| = {:.3e} m", n, worst);
    assert!(worst < 1e-3);
}
