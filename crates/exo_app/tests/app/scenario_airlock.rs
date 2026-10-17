use crate::common;
use exo_app::Options;

#[test]
fn airlock_scenario_passes_headless() {
    let o = Options { scenario: Some("airlock".into()), start: Some("dev_area".into()), warm_reactor: true, headless: true, out_dir: common::out_dir("airlock-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
