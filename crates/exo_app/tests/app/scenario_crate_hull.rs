//! Issue #210: a crate thrown at the ship hull stops outside it. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn crate_hull_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-hull".into()), headless: true, out_dir: common::out_dir("crate-hull-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
