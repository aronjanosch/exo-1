//! #90, #91: the boost capacitor and the minimal HUD, driven through `Controls`.
mod common;
use exo_app::Options;

#[test]
fn boost_hud_scenario_passes_headless() {
    let o = Options { scenario: Some("boost-hud".into()), headless: true, out_dir: std::env::temp_dir().join("exo-boost-hud-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
