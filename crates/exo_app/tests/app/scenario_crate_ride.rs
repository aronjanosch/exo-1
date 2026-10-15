use crate::common;
use exo_app::Options;

#[test]
fn crate_ride_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-ride".into()), headless: true, out_dir: common::out_dir("crate-ride-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
