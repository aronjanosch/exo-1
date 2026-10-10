//! Spike 13: the axis flight model flies a set of manoeuvres; the numbers go into a table.
use crate::common;
use exo_app::Options;

#[test]
fn flight_model_scenario_passes_headless() {
    let o = Options { scenario: Some("flight-model".into()), headless: true, out_dir: std::env::temp_dir().join("exo-flight-model-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
