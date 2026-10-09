//! Spike 11: two planets and the quantum drive between them, without a window.
mod common;
use exo_app::Options;

#[test]
fn warp_scenario_passes_headless() {
    let o = Options { scenario: Some("warp".into()), headless: true, out_dir: common::out_dir("warp-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
