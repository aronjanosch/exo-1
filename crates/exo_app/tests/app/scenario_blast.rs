//! The meltdown blast reaches the players. The dev drain melts the core with fuel in
//! it; the walker two metres off takes burn and radiation, the local flash is on, three fires start.
//! Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn blast_scenario_passes_headless() {
    let o = Options { scenario: Some("blast".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("blast-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
