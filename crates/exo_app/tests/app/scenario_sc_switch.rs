//! Round 5: F7 switches to the SC flight model, which lifts off, hovers and flies.
use crate::common;
use exo_app::Options;

#[test]
fn sc_switch_scenario_passes_headless() {
    let o = Options { scenario: Some("sc-switch".into()), headless: true, out_dir: common::out_dir("sc-switch-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
