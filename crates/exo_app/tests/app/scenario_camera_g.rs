use crate::common;
use exo_app::Options;

#[test]
fn camera_g_scenario_passes_headless() {
    let o = Options { scenario: Some("camera-g".into()), headless: true, out_dir: common::out_dir("camera-g-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
