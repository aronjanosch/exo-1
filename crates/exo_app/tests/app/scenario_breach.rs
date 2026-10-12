//! #286: hits, breaches and life support in the engineer's corner: a hit tears the hull and
//! starts a fire, the air falls, the door contains it, the breach is patched, the fire put out,
//! the generator refills the room, the machines are repaired. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn breach_scenario_passes_headless() {
    let o = Options { scenario: Some("breach".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("breach-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
