use crate::common;
use exo_app::Options;

#[test]
fn repair_scenario_passes_headless() {
    let o = Options { scenario: Some("repair".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("repair-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
