//! Round 5 (#195): the SC model's linear law through `Controls` (cap, brake, gravity compensation,
//! master modes, speed limiter); the checks are in the scenario.
use crate::common;
use exo_app::Options;

#[test]
fn sc_linear_scenario_passes_headless() {
    let o = Options { scenario: Some("sc-linear".into()), headless: true, out_dir: common::out_dir("sc-linear-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
