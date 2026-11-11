use crate::common;
use exo_app::Options;

#[test]
fn hand_lamp_scenario_passes_headless() {
    let o = Options { scenario: Some("hand_lamp".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("hand-lamp-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
