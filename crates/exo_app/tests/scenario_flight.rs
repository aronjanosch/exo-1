//! Sprint 2 feel (#24, #25, #26): input ramp, virtual-joystick mouse, boost, decoupled flight.
mod common;
use exo_app::Options;

#[test]
fn flight_scenario_passes_headless() {
    let o = Options { scenario: Some("flight".into()), headless: true, out_dir: std::env::temp_dir().join("exo-flight-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
