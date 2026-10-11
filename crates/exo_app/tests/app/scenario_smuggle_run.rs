//! Round 6: the whole loop: brew, fly, hail, scan, deal, rise. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn smuggle_run_scenario_passes_headless() {
    let o = Options { scenario: Some("smuggle_run".into()), headless: true, out_dir: common::out_dir("smuggle-run-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
