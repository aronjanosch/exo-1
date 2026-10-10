//! #177 (spike 14): the warp scenario on a planet of 30 km radius, with the ceilings that follow
//! from it (`--radius=`): the orbit start, the camera and the obstruction check follow the radius.
use crate::common;
use exo_app::Options;

#[test]
fn warp_scenario_passes_at_30_km() {
    let o = Options { scenario: Some("warp".into()), headless: true, radius: Some(30_000.0), out_dir: common::out_dir("radius-30km-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}

#[test]
fn warp_scenario_passes_at_100_km() {
    let o = Options { scenario: Some("warp".into()), headless: true, radius: Some(100_000.0), out_dir: common::out_dir("radius-100km-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}

/// The first run bakes the sites of a 300 km planet (about 100 s); the cache keeps them.
/// `cargo ts scenario_radius -- --run-ignored all`
#[test]
#[ignore]
fn warp_scenario_passes_at_300_km() {
    let o = Options { scenario: Some("warp".into()), headless: true, radius: Some(300_000.0), out_dir: common::out_dir("radius-300km-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
