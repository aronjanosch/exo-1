use crate::common;
use exo_app::Options;

#[test]
fn fire_scenario_passes_headless() {
    let o = Options { scenario: Some("fire".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("fire-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
