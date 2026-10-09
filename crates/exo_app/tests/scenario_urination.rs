//! Bladder cycle, random ballistic droplets, impacts and lifetime through Controls.
mod common;
use exo_app::Options;

#[test]
fn urination_scenario_passes_headless() {
    let o = Options { scenario: Some("urination".into()), headless: true, out_dir: std::env::temp_dir().join("exo-urination-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
