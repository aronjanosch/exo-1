//! Round 6: fill the kettle, brew, stop the needle, take the product. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn lab_scenario_passes_headless() {
    let o = Options { scenario: Some("lab".into()), headless: true, out_dir: common::out_dir("lab-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
