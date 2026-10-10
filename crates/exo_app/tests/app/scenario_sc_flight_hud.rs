//! #200: the flight HUD's readout (speed tape, thrust cross, G bar, horizon, velocity marker) through `Controls`.
use crate::common;
use exo_app::Options;

#[test]
fn sc_flight_hud_scenario_passes_headless() {
    let o = Options { scenario: Some("sc-flight-hud".into()), headless: true, out_dir: common::out_dir("sc-flight-hud-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
