use crate::common;
use exo_app::Options;

#[test]
fn equipment_scenario_passes_headless() {
    let o = Options { scenario: Some("equipment".into()), start: Some("dev_area".into()), warm_reactor: true, headless: true, out_dir: common::out_dir("equipment-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
