//! Sprint 2 feel (#24, #25, #26): input ramp, virtual-joystick mouse, boost, decoupled flight.
use crate::common;
use exo_app::Options;

#[test]
fn flight_scenario_passes_headless() {
    let o = Options { scenario: Some("flight".into()), headless: true, out_dir: common::out_dir("flight-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
