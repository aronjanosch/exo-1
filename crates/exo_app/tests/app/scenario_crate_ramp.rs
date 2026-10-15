use crate::common;
use exo_app::Options;

#[test]
fn crate_ramp_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-ramp".into()), headless: true, out_dir: common::out_dir("crate-ramp-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
