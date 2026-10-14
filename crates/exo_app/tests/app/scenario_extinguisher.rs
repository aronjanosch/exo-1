//! #300: the extinguisher wears down while it sprays, an empty one stops, and the corridor's rack
//! swaps it for a full spare. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn extinguisher_scenario_passes_headless() {
    let o = Options { scenario: Some("extinguisher".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("extinguisher-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
