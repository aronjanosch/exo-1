//! Issue #82: one tap for a crate and the seat, the HUD prompt says which. Without a window.
mod common;
use exo_app::Options;

#[test]
fn interact_scenario_passes_headless() {
    let o = Options { scenario: Some("interact".into()), headless: true, out_dir: std::env::temp_dir().join("exo-interact-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
