use crate::common;
use exo_app::Options;

#[test]
fn dig_scenario_passes_headless() {
    let o = Options { scenario: Some("dig".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("dig-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
