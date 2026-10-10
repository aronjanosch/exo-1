//! #177 (spike 14): the warp scenario on a planet of 30 km radius, with the ceilings that follow
//! from it (`--radius=`): the orbit start, the camera and the obstruction check follow the radius.
use crate::common;
use exo_app::Options;

#[test]
fn warp_scenario_passes_at_30_km() {
    let o = Options { scenario: Some("warp".into()), headless: true, radius: Some(30_000.0), out_dir: common::out_dir("radius-30km-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
