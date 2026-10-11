//! #177 (spike 14): skirts hide the cracks between quadtree levels. Where a chunk meets a chunk of
//! the next finer level, the surface of one lies above the other by some metres along the shared
//! edge; the skirt that hangs from the higher chunk's edge covers that gap when it is deeper than
//! the gap. Checked on the parent's bottom edge against its first child, at several radii.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn skirt_m(p: &Planet, face: usize, a0: f64, b0: f64, size: f64) -> f64 {
    (p.chunk_edge_m(face, a0, b0, size) / GRID as f64 * 4.0).max(2.0)
}

fn check(radius: f64, depth: u32) -> (usize, usize, f64, f64) {
    let text = HEARTH.replace("\"resolution\": 512", "\"resolution\": 128");
    let mut p = Planet::new(Recipe::for_planet(&text, 1337, radius).unwrap());
    p.bake_with(0, None, false);
    let size = 2.0 / (1u64 << depth) as f64;
    let (mut gaps, mut open, mut worst, mut biggest) = (0, 0, 0.0f64, 0.0f64);
    // A spread of parents over the six faces.
    for face in 0..6 {
        for k in 0..12 {
            let n = 1u64 << depth;
            let (ia, ib) = ((k * 7 + face as u64 * 3) % n, (k * 5 + face as u64) % n);
            let (a0, b0) = (-1.0 + ia as f64 * size, -1.0 + ib as f64 * size);
            let parent = p.build_chunk(face, a0, b0, size);
            let child = p.build_chunk(face, a0, b0, size / 2.0);
            let (sp, sc) = (skirt_m(&p, face, a0, b0, size), skirt_m(&p, face, a0, b0, size / 2.0));
            for i in 1..=GRID + 1 {
                let u = 1.0 + (i - 1) as f64 / 2.0;
                let (u0, f) = (u.floor() as usize, u - u.floor());
                let u1 = (u0 + 1).min(GRID + 1);
                let hp = parent.heights[M + u0] as f64 * (1.0 - f) + parent.heights[M + u1] as f64 * f;
                let hc = child.heights[M + i] as f64;
                let g = hp - hc; // > 0: the parent's edge is higher
                gaps += 1;
                biggest = biggest.max(g.abs());
                let covered = if g >= 0.0 { g <= sp } else { -g <= sc };
                if !covered {
                    open += 1;
                    worst = worst.max(g.abs());
                }
            }
        }
    }
    (gaps, open, worst, biggest)
}

#[test]
fn skirts_cover_the_gap_between_levels() {
    for (radius, depth) in [(5000.0, 4), (30_000.0, 6), (100_000.0, 8)] {
        let (gaps, open, worst, biggest) = check(radius, depth);
        println!("radius {radius}: {open} of {gaps} edge samples left open (worst {worst:.2} m), biggest level gap {biggest:.2} m");
        assert!(open * 100 <= gaps, "radius {radius}: {open} of {gaps} edge samples show a crack (worst {worst:.2} m)");
    }
}
