use crate::common;
use exo_app::Options;

#[test]
fn interior_look_scenario_passes_headless() {
    let o = Options { scenario: Some("interior_look".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("interior-look-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
