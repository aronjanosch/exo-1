//! Issue #84: the lock grid through hard acceleration and a warp; grabbing unlocks. Without a window.
mod common;
use exo_app::Options;

#[test]
fn crate_lock_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-lock".into()), headless: true, out_dir: common::out_dir("crate-lock-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
