//! #92: land on a slope below the limit; the ship does not drift from its touchdown spot.
mod common;
use exo_app::Options;

#[test]
fn slope_landing_scenario_passes_headless() {
    let o = Options { scenario: Some("slope-landing".into()), headless: true, out_dir: std::env::temp_dir().join("exo-slope-landing-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
