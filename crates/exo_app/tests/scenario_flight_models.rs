//! Spike 13: the classic and the axis flight model fly the same manoeuvres, switched with F7.
mod common;
use exo_app::Options;

#[test]
fn flight_models_scenario_passes_headless() {
    let o = Options { scenario: Some("flight-models".into()), headless: true, out_dir: std::env::temp_dir().join("exo-flight-models-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
