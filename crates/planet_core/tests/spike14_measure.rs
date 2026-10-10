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
        "| {radius_km:.0} | {:.1} | {:.0} | {:.0} | {:.0} | {:.1} | {:.0} / {:.0} | {} | {:.1} | {} | {} | {} | {chunks} | {height_us:.1} | {} |",
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
        format!("{:.0} / {:.0}", cold.sites_ms, warm.sites_ms),
    )
}

#[test]
#[ignore]
fn measure() {
    let radii: Vec<f64> = std::env::var("SPIKE14_RADII_KM").unwrap_or("5,30,100,300".into()).split(',').map(|s| s.parse().unwrap()).collect();
    println!("| radius km | coarse MB | Planet::new ms | first bake ms | cached load ms | RSS after bake MB | height range m | rivers | sea m | lakes | sites | landforms | chunk build ms by quadtree level | height_at us | site placement ms (cold / cached) |");
    for r in radii {
        println!("{}", row(r));
    }
}

/// The recipe without landforms and sites: the coarse layer alone (what the 1000 km dry run is).
fn without_content(text: &str) -> String {
    fn zero(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m.iter_mut() {
                    if k == "per_100_km2" {
                        *x = serde_json::json!([0.0, 0.0]);
                    } else {
                        zero(x);
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(zero),
            _ => {}
        }
    }
    let mut v: serde_json::Value = serde_json::from_str(text).unwrap();
    zero(&mut v);
    serde_json::to_string(&v).unwrap()
}

/// Size and bake time of the coarse layer alone at 1000 km (no landforms, no sites).
#[test]
#[ignore]
fn dry_run_1000_km() {
    let text = without_content(HEARTH);
    let dir = cache_dir().join("dry");
    let _ = std::fs::remove_dir_all(&dir);
    let radius = 1_000_000.0;
    let t = Instant::now();
    let mut p = Planet::new(Recipe::for_planet(&text, 1337, radius).unwrap());
    let new_ms = t.elapsed().as_secs_f64() * 1e3;
    let cold = p.bake_with(0, Some(&dir), false);
    let file = std::fs::read_dir(&dir).unwrap().next().unwrap().unwrap().metadata().unwrap().len();
    let t = Instant::now();
    let mut q = Planet::new(Recipe::for_planet(&text, 1337, radius).unwrap());
    let warm = q.bake_with(0, Some(&dir), false);
    assert!(warm.coarse_from_cache);
    println!(
        "| 1000 (coarse only) | {:.1} MB in memory, {:.1} MB on disk | Planet::new {new_ms:.0} ms | first bake {:.0} ms (coarse {:.0}, sea {:.0}, drainage {:.0}) | cached load {:.0} ms | rivers {} lakes {} | sea {:.1} m | cell {:.0} m |",
        cold.coarse_bytes as f64 / 1e6,
        file as f64 / 1e6,
        cold.bake_ms,
        cold.coarse_ms,
        cold.sea_ms + cold.macro_ms,
        cold.drainage_ms,
        t.elapsed().as_secs_f64() * 1e3 - new_ms.min(0.0),
        p.coarse.rivers.len(),
        p.coarse.lakes.len(),
        p.sea,
        std::f64::consts::FRAC_PI_2 * radius / 512.0,
    );
}

/// Gaps between sites and the angle between the first pickup and the first dropoff per radius.
#[test]
#[ignore]
fn site_gaps_and_trip() {
    let place = |lat: f64, lon: f64| {
        let (la, lo) = (lat.to_radians(), lon.to_radians());
        v3(la.cos() * lo.cos(), la.sin(), la.cos() * lo.sin())
    };
    let angle = place(86.0, 90.0).dot(place(70.0, 315.0)).clamp(-1.0, 1.0).acos();
    println!("Drip Rock -> Bent Spoon: {:.4} rad of arc", angle);
    for radius_km in [5.0, 30.0, 100.0] {
        let radius = radius_km * 1000.0;
        let text = HEARTH.replace("\"resolution\": 512", "\"resolution\": 128");
        let mut p = Planet::new(Recipe::for_planet(&text, 1337, radius).unwrap());
        let st = p.bake_with(0, None, true);
        let d = radius * angle;
        println!(
            "R {radius_km} km: pad to pad {:.1} km; sites {}; median nearest site {:.0} m, worst hole to land {:.0} m; at 150 m/s {:.1} min, 350 m/s {:.1} min, 785 m/s {:.1} min",
            d / 1000.0,
            st.site_count,
            st.site_median_nn_m,
            st.site_cover_worst_m,
            d / 150.0 / 60.0,
            d / 350.0 / 60.0,
            d / 785.0 / 60.0
        );
    }
}
