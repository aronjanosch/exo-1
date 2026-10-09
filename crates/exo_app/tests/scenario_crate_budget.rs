//! Issue #85: object budget: cap, sleep, persistence cap over a planet swap, timeout, distance. Without a window.
mod common;
use exo_app::Options;

#[test]
fn crate_budget_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-budget".into()), headless: true, out_dir: std::env::temp_dir().join("exo-crate-budget-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
