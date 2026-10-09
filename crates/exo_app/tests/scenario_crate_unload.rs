//! Night extra E1: unload the parked ship down the ramp by hand and load it again. Without a window.
mod common;
use exo_app::Options;

#[test]
fn crate_unload_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-unload".into()), headless: true, out_dir: common::out_dir("crate-unload-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
