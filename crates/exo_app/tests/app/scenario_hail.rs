//! Round 6: hail the tower, land on the assigned pad, a landing without clearance. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn hail_scenario_passes_headless() {
    let o = Options { scenario: Some("hail".into()), headless: true, out_dir: common::out_dir("hail-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
