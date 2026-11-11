use crate::common;
use exo_app::Options;

#[test]
fn coupled_loop_scenario_passes_headless() {
    let o = Options { scenario: Some("coupled-loop".into()), headless: true, out_dir: common::out_dir("coupled-loop-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
