use crate::common;
use exo_app::Options;

#[test]
fn crate_budget_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-budget".into()), headless: true, out_dir: common::out_dir("crate-budget-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
