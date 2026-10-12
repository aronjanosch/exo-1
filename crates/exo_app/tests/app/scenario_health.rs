//! #287, #254: health on the dev area: fire burns, the lab's salve treats, no air kills, respawn
//! at the pads with a skill loss. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn health_scenario_passes_headless() {
    let o = Options { scenario: Some("health".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("health-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
