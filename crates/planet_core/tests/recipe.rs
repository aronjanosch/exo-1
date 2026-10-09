//! #64: one recipe per planet, data only, unknown fields rejected.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");
const CINDER: &str = include_str!("../../../content/planet/cinder.json");

#[test]
fn both_recipes_load_with_the_planets_seed_and_radius() {
    for (text, seed) in [(HEARTH, 1337), (CINDER, 4242)] {
        let r = Recipe::for_planet(text, seed, 5000.0).unwrap();
        assert_eq!((r.seed, r.radius), (seed, 5000.0));
    }
}

fn with(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "fixture: {from}");
    text.replacen(from, to, 1)
}

#[test]
fn unknown_fields_are_rejected_at_every_level() {
    let top = with(HEARTH, "\"macro\":", "\"colour_of_the_wind\": 1, \"macro\":");
    let nested = with(HEARTH, "\"land_fraction\": 0.66", "\"land_fraction\": 0.66, \"tide\": 2");
    let stamp = with(HEARTH, "\"depth_m\": [80.0, 100.0]", "\"depth_m\": [80.0, 100.0], \"wobble\": 1");
    // Seed and radius belong to system.json.
    let seed = with(HEARTH, "\"macro\":", "\"seed\": 7, \"macro\":");
    for (what, t) in [("top", top), ("nested", nested), ("stamp", stamp), ("seed", seed)] {
        let e = Recipe::from_json(&t).expect_err(what);
        assert!(e.contains("unknown field"), "{what}: {e}");
    }
}
