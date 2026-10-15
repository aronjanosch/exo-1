use crate::common;
use exo_app::Options;

#[test]
fn coolant_scenario_passes_headless() {
    let o = Options { scenario: Some("coolant".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("coolant-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
