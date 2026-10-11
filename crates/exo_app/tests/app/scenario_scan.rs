//! Round 6: inspection at high heat: comply, the hidden hold, fleeing. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn scan_scenario_passes_headless() {
    let o = Options { scenario: Some("scan".into()), headless: true, out_dir: common::out_dir("scan-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
