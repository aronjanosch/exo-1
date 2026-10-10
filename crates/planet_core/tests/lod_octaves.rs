//! Octave culling by level of detail (#216): a noise octave whose wavelength is below about two
//! vertex spacings is faded out (over one octave), so a coarse chunk does not sample detail its
//! grid cannot hold. The finest chunks keep every octave.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

#[test]
fn octave_weight_is_full_from_two_spacings_and_gone_at_one() {
    assert_eq!(octave_weight(2.0, 1.0), 1.0);
    assert_eq!(octave_weight(19.0, 1.0), 1.0);
    assert_eq!(octave_weight(1.0, 1.0), 0.0);
    assert_eq!(octave_weight(0.5, 1.0), 0.0);
    // One octave of fade, no step: the weight rises monotonically in small increments.
    let mut prev = 0.0;
    for k in 0..=2000 {
        let w = octave_weight(1.0 + k as f64 / 2000.0, 1.0);
        assert!((0.0..=1.0).contains(&w));
        assert!(w >= prev, "not monotone at k = {k}");
        assert!(w - prev < 0.01, "step of {} at k = {k}", w - prev);
        prev = w;
    }
}

#[test]
fn finest_chunks_keep_every_octave() {
    // 100 km planet, finest quadtree level (about 1 m vertex spacing), as the chunk timing harness.
    let mut p = Planet::new(Recipe::for_planet(HEARTH, 1337, 100_000.0).unwrap());
    p.bake_with(0, None, false);
    let face_edge = std::f64::consts::FRAC_PI_2 * p.radius;
    let depth = (face_edge / 37.0).log2().round() as u32;
    let n = 1u64 << depth;
    let size = 2.0 / n as f64;
    let mut worst: f64 = 0.0;
    for k in 0..8u64 {
        let face = (k % 6) as usize;
        let (a0, b0) = (-1.0 + ((k * 97 + 11) % n) as f64 * size, -1.0 + ((k * 53 + 7) % n) as f64 * size);
        let spacing = p.chunk_edge_m(face, a0, b0, size) / GRID as f64;
        assert!(spacing < 1.5, "the finest level has a spacing of {spacing} m");
        let lod = p.build_chunk(face, a0, b0, size);
        let full = p.build_chunk_unculled(face, a0, b0, size);
        for (x, y) in lod.heights.iter().zip(&full.heights) {
            worst = worst.max((*x as f64 - *y as f64).abs());
        }
    }
    println!("finest chunks: worst |culled - full| = {worst:.3e} m");
    assert!(worst < 1e-3, "worst {worst:.3e} m");
}

#[test]
fn coarse_chunks_drop_the_octaves_they_cannot_hold() {
    let mut p = Planet::new(Recipe::for_planet(HEARTH, 1337, 100_000.0).unwrap());
    p.bake_with(0, None, false);
    let face_edge = std::f64::consts::FRAC_PI_2 * p.radius;
    let depth = (face_edge / 37.0).log2().round() as u32 / 2;
    let n = 1u64 << depth;
    let size = 2.0 / n as f64;
    let mut worst: f64 = 0.0;
    let mut differs = 0usize;
    for k in 0..8u64 {
        let face = (k % 6) as usize;
        let (a0, b0) = (-1.0 + ((k * 97 + 11) % n) as f64 * size, -1.0 + ((k * 53 + 7) % n) as f64 * size);
        let lod = p.build_chunk(face, a0, b0, size);
        let full = p.build_chunk_unculled(face, a0, b0, size);
        for (x, y) in lod.heights.iter().zip(&full.heights) {
            let d = (*x as f64 - *y as f64).abs();
            worst = worst.max(d);
            differs += (d > 1e-3) as usize;
        }
    }
    println!("middle chunks: worst |culled - full| = {worst:.3} m, {differs} vertices differ");
    assert!(differs > 0, "culling changes nothing at the middle level");
    assert!(worst < 100.0, "worst {worst:.3} m");
}
