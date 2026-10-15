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

#[test]
fn relief_is_repeatable_at_100_km() {
    let (a, _) = shared("Hearth");
    let (b, _) = baked(HEARTH, 1337);
    let sa = serde_json::to_string(&a.shape_stats(4000, 0)).unwrap();
    let sb = serde_json::to_string(&b.shape_stats(4000, 0)).unwrap();
    assert_eq!(sa, sb);
}

