use planet_core::*;
use std::sync::OnceLock;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");
const CINDER: &str = include_str!("../../../content/planet/cinder.json");
const RADIUS_M: f64 = 100_000.0;

/// A 100 km planet with its ceiling set as the app sets it for `--radius` (the tallest terrain
/// of its atmosphere), baked with all cores.
fn baked(json: &str, seed: i32) -> (Planet, BakeStats) {
    let mut p = Planet::new(Recipe::for_planet(json, seed, RADIUS_M).unwrap());
    p.ceiling_m = Ceilings::for_radius(RADIUS_M).terrain_max;
    let st = p.bake(0);
    (p, st)
}

/// Each planet is baked once for this test binary and shared by its tests.
static HEARTH_BAKE: OnceLock<(Planet, BakeStats)> = OnceLock::new();
static CINDER_BAKE: OnceLock<(Planet, BakeStats)> = OnceLock::new();

fn shared(name: &str) -> &'static (Planet, BakeStats) {
    match name {
        "Hearth" => HEARTH_BAKE.get_or_init(|| baked(HEARTH, 1337)),
        _ => CINDER_BAKE.get_or_init(|| baked(CINDER, 4242)),
    }
}

fn assert_shape(name: &str) {
    let (p, st) = shared(name);
    let sh = p.shape_stats(40_000, 0);
    println!(
        "{name}: tallest {:.1} m above radius, sea {:.2} m, plain {:.1} % of land, land {:.1} % (bake {:.1} %), biomes {}, lakes {:.1} % of the surface, river nodes {}",
        sh.max_above_radius_m,
        st.sea_level_m,
        sh.plain_share_land * 100.0,
        sh.land_share * 100.0,
        st.land_fraction_full * 100.0,
        sh.biomes_present,
        st.lake_area_share * 100.0,
        st.river_nodes
    );
    assert!((900.0..=1170.0).contains(&sh.max_above_radius_m), "{name}: tallest {} m", sh.max_above_radius_m);
    assert!((0.20..=0.40).contains(&sh.plain_share_land), "{name}: plain {}", sh.plain_share_land);
    assert!((sh.land_share - st.land_fraction_full).abs() <= 0.05, "{name}: land {} vs bake {}", sh.land_share, st.land_fraction_full);
    assert!(sh.biomes_present >= 6, "{name}: biomes {:?}", sh.biome_share_land);
    // Ground above 1100 m stays rare: the soft ceiling's knee is where the tallest ridges bend.
    let high = land_share_above(p, 1100.0, 40_000);
    println!("{name}: land above 1100 m {:.2} %", high * 100.0);
    assert!(high < 0.005, "{name}: {:.2} % of the land above 1100 m", high * 100.0);
}

/// Share of the land (dry, above the sea) higher than `m` metres, over `n` points of a Fibonacci
/// lattice of the sphere (the same lattice as the shape statistics).
fn land_share_above(p: &Planet, m: f64, n: usize) -> f64 {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    let (mut land, mut high) = (0usize, 0usize);
    for i in 0..n {
        let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let th = golden * i as f64;
        let s = p.sample(v3(r * th.cos(), y, r * th.sin()));
        if s.height_above_sea > 0.0 && s.water_depth <= 0.0 {
            land += 1;
            if s.height_above_sea > m {
                high += 1;
            }
        }
    }
    high as f64 / land.max(1) as f64
}

#[test]
fn hearth_has_readable_relief_at_100_km() {
    assert_shape("Hearth");
}

#[test]
fn cinder_has_readable_relief_at_100_km() {
    assert_shape("Cinder");
}

#[test]
fn relief_is_repeatable_at_100_km() {
    let (a, _) = shared("Hearth");
    let (b, _) = baked(HEARTH, 1337);
    let sa = serde_json::to_string(&a.shape_stats(4000, 0)).unwrap();
    let sb = serde_json::to_string(&b.shape_stats(4000, 0)).unwrap();
    assert_eq!(sa, sb);
}

/// The biome rows at 100 km. No row takes more than 30 % of the land, the snow row only the
/// high ground (10 % at most), and six or more rows show.
#[test]
fn hearth_biome_rows_share_the_land_at_100_km() {
    let (p, _) = shared("Hearth");
    let sh = p.shape_stats(40_000, 0);
    println!("Hearth biome shares of land: {:?}", sh.biome_share_land);
    for (row, share) in &sh.biome_share_land {
        assert!(*share <= 0.30, "Hearth: row {row} takes {:.1} % of the land", share * 100.0);
    }
    let cold = sh.biome_share_land.get(&3).copied().unwrap_or(0.0);
    assert!(cold <= 0.10, "Hearth: the cold row takes {:.1} % of the land", cold * 100.0);
    assert!(sh.biomes_present >= 6, "Hearth: {} rows present", sh.biomes_present);
}

/// The instances of 24 cells of 0.3 km (4 per face, Hearth at 100 km) per storey. The
/// density budget: the foliage may add at most 30 % over the count below.
fn sample_instances(p: &Planet) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (st, (name, _)) in p.storeys().iter().enumerate() {
        let mut n = 0;
        for face in 0..6 {
            for (a0, b0) in [(-0.5, -0.5), (-0.5, 0.5), (0.5, -0.5), (0.5, 0.5)] {
                n += p.build_scatter(face, a0, b0, 2.0 / 512.0, st).instances.len();
            }
        }
        out.push((name.clone(), n));
    }
    out
}

/// The count recorded here (Hearth, 100 km, the same 24 cells): 114062 instances.
const SCATTER_BEFORE_L6: f64 = 114_062.0;

#[test]
fn hearth_scatter_density_at_100_km_stays_in_budget() {
    let (p, _) = shared("Hearth");
    let counts = sample_instances(p);
    let total: usize = counts.iter().map(|(_, n)| n).sum();
    println!("Hearth instances in 24 cells of 0.3 km: {counts:?}, total {total}");
    assert!(total as f64 <= 1.3 * SCATTER_BEFORE_L6, "{total} instances, budget {}", 1.3 * SCATTER_BEFORE_L6);
}
