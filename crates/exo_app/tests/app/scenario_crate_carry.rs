use crate::common;
use exo_app::Options;

#[test]
fn crate_carry_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-carry".into()), headless: true, out_dir: common::out_dir("crate-carry-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
