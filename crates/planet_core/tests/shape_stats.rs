//! #214: terrain shape statistics (plain share, tallest ground, biomes) on Hearth at 5 km.
use planet_core::*;

fn hearth() -> (Planet, BakeStats) {
    let r = Recipe::for_planet(include_str!("../../../content/planet/hearth.json"), 1337, 5000.0).unwrap();
    let mut p = Planet::new(r);
    let st = p.bake(0);
    (p, st)
}

#[test]
fn shape_stats_are_consistent_and_repeatable() {
    let (p, st) = hearth();
    let a = p.shape_stats(4000, 0);
    let b = p.shape_stats(4000, 3);
    assert_eq!(a.samples, 4000);
    // Same lattice on any thread count.
    assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
    // The bake's own land share (full height function) within lattice noise; lakes and rivers
    // count as land there and as water here, so dry land may lie a little below.
    assert!(a.land_share <= st.land_fraction_full + 0.02 && a.land_share > st.land_fraction_full - 0.05, "land {} vs bake {}", a.land_share, st.land_fraction_full);
    assert!((0.0..=1.0).contains(&a.plain_share_land));
    assert!(a.max_above_sea_m >= a.p99_land_above_sea_m && a.p99_land_above_sea_m > 0.0);
    let total: f64 = a.biome_share_land.values().sum();
    assert!((total - 1.0).abs() < 1e-9, "biome shares sum to {total}");
    assert!(a.biomes_present >= 3, "{:?}", a.biome_share_land);
}

#[test]
fn a_flat_patch_counts_as_plain_and_a_steep_one_does_not() {
    // Hearth at 5 km has both: some land must be plain, not all of it.
    let s = hearth().0.shape_stats(4000, 0);
    assert!(s.plain_share_land > 0.02 && s.plain_share_land < 0.98, "plain {}", s.plain_share_land);
}
