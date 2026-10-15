use crate::common;
use exo_app::Options;

#[test]
fn swap_scenario_passes_headless() {
    let o = Options { scenario: Some("swap".into()), headless: true, out_dir: common::out_dir("swap-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
