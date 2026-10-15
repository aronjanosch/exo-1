use crate::common;
use exo_app::Options;

#[test]
fn fire_grid_scenario_passes_headless() {
    let o = Options { scenario: Some("fire_grid".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("fire-grid-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
