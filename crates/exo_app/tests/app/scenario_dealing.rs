//! Round 6: a buyer's order by effect, a counter-offer, the handover. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn dealing_scenario_passes_headless() {
    let o = Options { scenario: Some("dealing".into()), headless: true, out_dir: common::out_dir("dealing-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
