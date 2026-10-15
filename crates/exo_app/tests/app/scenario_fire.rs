//! Fire at the reactor: it grows and uses the air, the alarm follows its size, a walker beside
//! it catches fire, the pump, the junction box and the reactor lose condition, and the extinguisher
//! puts the three fires out; the alarm is back to none. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn fire_scenario_passes_headless() {
    let o = Options { scenario: Some("fire".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("fire-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
