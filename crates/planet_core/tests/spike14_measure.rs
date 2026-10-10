//! Spike 14 (#177): the headless measurements per radius. Ignored by default; run
//! `cargo test --release -p planet_core --test spike14_measure -- --ignored --nocapture`
//! (`SPIKE14_RADII_KM=30,100,300` picks the radii). One markdown row per radius.
use planet_core::*;
use std::time::Instant;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

/// Resident memory of this process in MB (Linux: /proc/self/statm).
fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1)?.parse::<f64>().ok()).map_or(0.0, |p| p * 4096.0 / 1e6)
}

fn cache_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("spike14-cache")
}

fn row(radius_km: f64) -> String {
    let radius = radius_km * 1000.0;
    let dir = cache_dir();
    let rss0 = rss_mb();
    let t = Instant::now();
    let mut p = Planet::new(Recipe::for_planet(HEARTH, 1337, radius).unwrap());
    let new_ms = t.elapsed().as_secs_f64() * 1e3;
    let _ = std::fs::remove_dir_all(&dir);
    let cold = p.bake_with(0, Some(&dir), false);
    let rss1 = rss_mb();
    let cold_total = cold.bake_ms;
    drop(p);
    let t = Instant::now();
    let mut q = Planet::new(Recipe::for_planet(HEARTH, 1337, radius).unwrap());
    let warm = q.bake_with(0, Some(&dir), false);
    let warm_total = t.elapsed().as_secs_f64() * 1e3;
    assert!(warm.coarse_from_cache);
    // Chunks: the six roots, then a descent to the finest level (about 37 m) at one point.
    let face_edge = std::f64::consts::FRAC_PI_2 * radius;
    let max_depth = (face_edge / 37.0).log2().round() as i32;
    let d = v3(0.3, 0.8, 0.52).normalized();
    let face = face_of(d);
    let (a, b) = sphere_to_face_ab(face, d);
    let mut times = Vec::new();
    for depth in [0, 2, 4, 6, 8, max_depth] {
        let size = 2.0 / (1u64 << depth) as f64;
        let (a0, b0) = (((a + 1.0) / size).floor() * size - 1.0, ((b + 1.0) / size).floor() * size - 1.0);
        let t = Instant::now();
        let reps = 5;
        for _ in 0..reps {
            std::hint::black_box(q.build_chunk(face, a0, b0, size));
        }
        times.push((depth, t.elapsed().as_secs_f64() * 1e3 / reps as f64));
    }
    let t = Instant::now();
    let reps = 20_000;
    let mut acc = 0.0;
    for k in 0..reps {
        let x = k as f64 * 0.37;
        acc += q.height_at(look::walk(d, v3(0.0, 0.0, 1.0).cross(d).normalized(), x, radius));
    }
    std::hint::black_box(acc);
    let height_us = t.elapsed().as_secs_f64() * 1e6 / reps as f64;
    let chunks = times.iter().map(|(d, ms)| format!("L{d} {ms:.1}")).collect::<Vec<_>>().join(", ");
    let (lo, hi) = q.height_range;
    format!(
        "| {radius_km:.0} | {:.1} | {:.0} | {:.0} | {:.0} | {:.1} | {:.0} / {:.0} | {} | {:.1} | {} | {} | {} | {chunks} | {height_us:.1} | {:.0} |",
        cold.coarse_bytes as f64 / 1e6,
        new_ms,
        cold_total,
        warm_total,
        rss1 - rss0,
        lo - q.sea,
        hi - q.sea,
        q.coarse.rivers.len(),
        q.sea,
        q.coarse.lakes.len(),
        q.sites.len(),
        q.stamps().len(),
        warm.sites_ms,
    )
}

#[test]
#[ignore]
fn measure() {
    let radii: Vec<f64> = std::env::var("SPIKE14_RADII_KM").unwrap_or("5,30,100,300".into()).split(',').map(|s| s.parse().unwrap()).collect();
    println!("| radius km | coarse MB | Planet::new ms | first bake ms | cached load ms | RSS after bake MB | height range m | rivers | sea m | lakes | sites | landforms | chunk build ms by quadtree level | height_at us | site placement ms |");
    for r in radii {
        println!("{}", row(r));
    }
}
