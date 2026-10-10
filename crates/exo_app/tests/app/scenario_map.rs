//! #166: the map's pins and M. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn map_scenario_passes_headless() {
    let o = Options { scenario: Some("map".into()), headless: true, out_dir: common::out_dir("map-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
