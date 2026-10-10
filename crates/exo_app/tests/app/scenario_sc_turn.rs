//! Round 5, #196: the SC model's rotation (overshoot, reversal, roll release, G-safe turn cap).
use crate::common;
use exo_app::Options;

#[test]
fn sc_turn_scenario_passes_headless() {
    let o = Options { scenario: Some("sc-turn".into()), headless: true, out_dir: common::out_dir("sc-turn-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
