//! Issue #80: a crate in the cabin through take-off, flight, warp and landing; one pushed out
//! over the ramp keeps the ship velocity at the hand-over. Without a window.
mod common;
use exo_app::Options;

#[test]
fn crate_ride_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-ride".into()), headless: true, out_dir: std::env::temp_dir().join("exo-crate-ride-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
