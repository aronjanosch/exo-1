//! Round 5: the SC flight model lifts off, hovers and flies.
use crate::common;
use exo_app::Options;

#[test]
fn sc_lift_scenario_passes_headless() {
    let o = Options { scenario: Some("sc-lift".into()), headless: true, out_dir: common::out_dir("sc-lift-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
