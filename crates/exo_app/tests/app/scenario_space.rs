use crate::common;
use exo_app::Options;

#[test]
fn space_scenario_passes_headless() {
    let o = Options { scenario: Some("space".into()), headless: true, out_dir: common::out_dir("space-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
