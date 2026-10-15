use crate::common;
use exo_app::Options;

#[test]
fn items_scenario_passes_headless() {
    let o = Options { scenario: Some("items".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("items-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
