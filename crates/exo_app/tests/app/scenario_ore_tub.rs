use crate::common;
use exo_app::Options;

#[test]
fn ore_tub_scenario_passes_headless() {
    let o = Options { scenario: Some("ore_tub".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("ore-tub-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
