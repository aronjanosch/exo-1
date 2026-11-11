use crate::common;
use exo_app::Options;

#[test]
fn dig_save_scenario_passes_headless() {
    let o = Options { scenario: Some("dig_save".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("dig-save-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
