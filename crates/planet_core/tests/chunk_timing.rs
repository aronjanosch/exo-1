//! Chunk build timing at a 100 km planet: 40 chunks at the root level, a middle level and the
//! finest level. Ignored by default; run
//! `cargo test --release -p planet_core --test chunk_timing -- --ignored --nocapture`.
use planet_core::*;
use std::time::Instant;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");
const RADIUS_M: f64 = 100_000.0;
const CHUNKS: usize = 40;

/// Builds `CHUNKS` chunks spread over the six faces at one quadtree depth and returns
/// (µs per chunk, µs per vertex, chunk spacing in metres).
fn time_level(p: &Planet, depth: u32) -> (f64, f64, f64) {
    let n = 1u64 << depth;
    let size = 2.0 / n as f64;
    let mut picks = Vec::with_capacity(CHUNKS);
    for k in 0..CHUNKS {
        let face = k % 6;
        let ia = (k as u64 * 7 + 3) % n;
        let ib = (k as u64 * 5 + 1) % n;
        picks.push((face, -1.0 + ia as f64 * size, -1.0 + ib as f64 * size));
    }
    // One warm-up build so the first chunk does not pay for cold caches.
    std::hint::black_box(p.build_chunk(picks[0].0, picks[0].1, picks[0].2, size));
    let t = Instant::now();
    for &(face, a0, b0) in &picks {
        std::hint::black_box(p.build_chunk(face, a0, b0, size));
    }
    let us_chunk = t.elapsed().as_secs_f64() * 1e6 / CHUNKS as f64;
    let spacing = p.chunk_edge_m(picks[0].0, picks[0].1, picks[0].2, size) / GRID as f64;
    let verts = (M * M) as f64;
    (us_chunk, us_chunk / verts, spacing)
}

#[test]
#[ignore]
fn chunk_build_timing() {
    let recipe = Recipe::for_planet(HEARTH, 1337, RADIUS_M).unwrap();
    let mut p = Planet::new(recipe);
    p.bake_with(0, None, false);
    let face_edge = std::f64::consts::FRAC_PI_2 * RADIUS_M;
    let finest = (face_edge / 37.0).log2().round() as u32;
    let middle = finest / 2;
    println!("radius {RADIUS_M} m, {CHUNKS} chunks per level, M*M = {} vertices", M * M);
    println!("| level | depth | spacing m | µs per chunk | µs per vertex |");
    println!("|---|---|---|---|---|");
    for (name, depth) in [("root", 0), ("middle", middle), ("finest", finest)] {
        let (us_chunk, us_vert, spacing) = time_level(&p, depth);
        println!("| {name} | {depth} | {spacing:.2} | {us_chunk:.1} | {us_vert:.3} |");
    }
}
