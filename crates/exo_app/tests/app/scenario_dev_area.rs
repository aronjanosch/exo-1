use crate::common;
use exo_app::Options;

#[test]
fn dev_area_scenario_passes_headless() {
    let o = Options { scenario: Some("dev_area".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("dev-area-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
