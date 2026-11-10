use crate::common;
use exo_app::Options;

#[test]
fn fabricator_scenario_passes_headless() {
    let o = Options { scenario: Some("fabricator".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("fabricator-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
