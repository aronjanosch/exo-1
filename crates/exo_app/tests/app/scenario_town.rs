//! Round 6: the city, its colliders and characters, the new towns' pads.
//! Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn town_scenario_passes_headless() {
    let o = Options { scenario: Some("town".into()), headless: true, out_dir: common::out_dir("town-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
