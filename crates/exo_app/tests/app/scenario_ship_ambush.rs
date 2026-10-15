use crate::common;
use exo_app::Options;

#[test]
fn ship_ambush_scenario_passes_headless() {
    let o = Options { scenario: Some("ship_ambush".into()), start: Some("dev_area".into()), warm_reactor: true, headless: true, out_dir: common::out_dir("ship-ambush-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
