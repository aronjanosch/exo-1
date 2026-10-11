//! #220: the morph target of a chunk is the parent's mesh. At full morph a child has exactly the
//! parent's surface (positions), so a split shows no pop.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn planet(radius: f64) -> Planet {
    let text = HEARTH.replace("\"resolution\": 512", "\"resolution\": 128");
    let mut p = Planet::new(Recipe::for_planet(&text, 1337, radius).unwrap());
    p.bake_with(0, None, false);
    p
}

fn abs(c: &ChunkOut, k: usize) -> [f64; 3] {
    let v = c.verts[k];
    [c.center[0] + v[0] as f64, c.center[1] + v[1] as f64, c.center[2] + v[2] as f64]
}

/// The parent mesh's surface at grid coordinates `(u, v)` (in cells, 1 = first real vertex),
/// on the triangle the shared index pattern gives.
fn parent_at(c: &ChunkOut, u: f64, v: f64) -> [f64; 3] {
    let (i, j) = (u.floor() as usize, v.floor() as usize);
    let (i, j) = (i.min(GRID), j.min(GRID));
    let (fx, fy) = (u - i as f64, v - j as f64);
    let p = |di: usize, dj: usize| abs(c, (j + dj) * M + i + di);
    let (p00, p10, p01, p11) = (p(0, 0), p(1, 0), p(0, 1), p(1, 1));
    let mut r = [0.0; 3];
    for a in 0..3 {
        r[a] = if fx + fy <= 1.0 { p00[a] + fx * (p10[a] - p00[a]) + fy * (p01[a] - p00[a]) } else { p11[a] + (1.0 - fx) * (p01[a] - p11[a]) + (1.0 - fy) * (p10[a] - p11[a]) };
    }
    r
}

#[test]
fn a_child_at_full_morph_is_the_parent_mesh() {
    let p = planet(30_000.0);
    let size = 0.25;
    let mut worst: f64 = 0.0;
    for (face, a0, b0) in [(0, -0.5, 0.25), (3, 0.0, -0.25), (5, 0.25, 0.5)] {
        let parent = p.build_chunk(face, a0, b0, size);
        for (ia, ib) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let child = p.build_chunk(face, a0 + ia as f64 * size / 2.0, b0 + ib as f64 * size / 2.0, size / 2.0);
            assert_eq!(child.morph_delta.len(), M * M);
            for j in 1..=GRID + 1 {
                for i in 1..=GRID + 1 {
                    let k = j * M + i;
                    let (own, d) = (abs(&child, k), child.morph_delta[k]);
                    let target = [own[0] + d[0] as f64, own[1] + d[1] as f64, own[2] + d[2] as f64];
                    let u = 1.0 + ((i - 1) as f64 + (GRID * ia) as f64) / 2.0;
                    let v = 1.0 + ((j - 1) as f64 + (GRID * ib) as f64) / 2.0;
                    let want = parent_at(&parent, u, v);
                    let e = (0..3).map(|a| (target[a] - want[a]).powi(2)).sum::<f64>().sqrt();
                    worst = worst.max(e);
                }
            }
        }
    }
    assert!(worst < 0.02, "child target vs parent mesh: worst {worst} m");
}

#[test]
fn even_vertices_do_not_morph_and_skirts_follow_their_edge() {
    let p = planet(30_000.0);
    let c = p.build_chunk(2, -0.3, 0.1, 0.0625);
    for j in (1..=GRID + 1).step_by(2) {
        for i in (1..=GRID + 1).step_by(2) {
            let d = c.morph_delta[j * M + i];
            assert!(d.iter().all(|x| x.abs() < 1e-3), "even vertex moved: {d:?}");
        }
    }
    // A skirt vertex hangs below the edge vertex and moves with it.
    for i in 0..M {
        let (skirt, edge) = (c.morph_delta[i], c.morph_delta[M + i.clamp(1, M - 2)]);
        for a in 0..3 {
            assert!((skirt[a] - edge[a]).abs() < 1e-3);
        }
    }
    // Normals are unit and point up-ish.
    for n in &c.morph_normals {
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((l - 1.0).abs() < 1e-3);
    }
}
