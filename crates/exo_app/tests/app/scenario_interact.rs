//! Issue #82: one tap for a crate and the seat, the HUD prompt says which. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn interact_scenario_passes_headless() {
    let o = Options { scenario: Some("interact".into()), headless: true, out_dir: common::out_dir("interact-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
