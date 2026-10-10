//! Round 5 (#199): the SC model's air. Wind compensation holds the hover, turbulence near the
//! ground, none at 1000 m.
use crate::common;
use exo_app::Options;

#[test]
fn sc_air_scenario_passes_headless() {
    let o = Options { scenario: Some("sc-air".into()), headless: true, out_dir: common::out_dir("sc-air-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
