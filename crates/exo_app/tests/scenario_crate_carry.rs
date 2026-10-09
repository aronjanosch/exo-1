//! Issue #83: carry each size, throw, the grab tool from 8 m, the large crate alone. Without a window.
mod common;
use exo_app::Options;

#[test]
fn crate_carry_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-carry".into()), headless: true, out_dir: std::env::temp_dir().join("exo-crate-carry-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
