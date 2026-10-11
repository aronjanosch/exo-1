//! Round 6: deeds, tribute, rank, the shop. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn empire_scenario_passes_headless() {
    let o = Options { scenario: Some("empire".into()), headless: true, out_dir: common::out_dir("empire-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
