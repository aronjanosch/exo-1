//! Round 6: an air patrol orders the hot ship to halt; halting gets a scan in the air, running costs heat. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn patrol_scenario_passes_headless() {
    let o = Options { scenario: Some("patrol".into()), headless: true, out_dir: common::out_dir("patrol-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
