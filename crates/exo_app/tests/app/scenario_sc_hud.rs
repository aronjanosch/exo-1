use crate::common;
use exo_app::Options;

#[test]
fn sc_hud_scenario_passes_headless() {
    let o = Options { scenario: Some("sc-hud".into()), headless: true, out_dir: common::out_dir("sc-hud-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
