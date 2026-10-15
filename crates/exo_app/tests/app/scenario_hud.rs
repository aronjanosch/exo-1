use crate::common;
use exo_app::Options;

#[test]
fn hud_scenario_passes_headless() {
    let o = Options { scenario: Some("hud".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("hud-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
