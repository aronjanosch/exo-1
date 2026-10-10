//! #221: the relief of the 100 km round. Hearth and Cinder at 100 km: the tallest ground, the plain
//! share, the land share against the bake, the biome count (the statistics of #214), and the
//! same statistics on a second bake (determinism).
use planet_core::*;

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

fn assert_shape(name: &str, json: &str, seed: i32) {
    let (p, st) = baked(json, seed);
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
}

#[test]
fn hearth_has_readable_relief_at_100_km() {
    assert_shape("Hearth", HEARTH, 1337);
}

#[test]
fn cinder_has_readable_relief_at_100_km() {
    assert_shape("Cinder", CINDER, 4242);
}

#[test]
fn relief_is_repeatable_at_100_km() {
    let (a, _) = baked(HEARTH, 1337);
    let (b, _) = baked(HEARTH, 1337);
    let sa = serde_json::to_string(&a.shape_stats(4000, 0)).unwrap();
    let sb = serde_json::to_string(&b.shape_stats(4000, 0)).unwrap();
    assert_eq!(sa, sb);
}
