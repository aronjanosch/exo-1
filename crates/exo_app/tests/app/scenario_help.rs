use crate::common;
use exo_app::Options;

#[test]
fn help_scenario_passes_headless() {
    let o = Options { scenario: Some("help".into()), headless: true, out_dir: common::out_dir("help-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
