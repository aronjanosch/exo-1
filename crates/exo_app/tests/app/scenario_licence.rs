use crate::common;
use exo_app::Options;

#[test]
fn licence_scenario_passes_headless() {
    let o = Options { scenario: Some("licence".into()), headless: true, out_dir: common::out_dir("licence-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
