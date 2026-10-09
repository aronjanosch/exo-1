//! #63: the planet-look scenario headless writes an atlas and statistics for every planet.
mod common;
use exo_app::Options;

#[test]
fn planet_look_headless_writes_atlas_and_stats() {
    let out = common::out_dir("look-scenario");
    let o = Options { scenario: Some("planet-look".into()), headless: true, out_dir: out.clone(), ..Default::default() };
    let _ = common::run_scenario(&o);
    for planet in ["hearth", "cinder"] {
        for f in ["atlas-height.png", "atlas-biome.png", "atlas-landform.png", "atlas-scatter.png", "stats.json"] {
            assert!(out.join("look").join(planet).join(f).exists(), "{planet}/{f} missing");
        }
    }
}
