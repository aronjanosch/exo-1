//! Fuel rods by hand. A rod from the corridor rack is loaded into the reactor (LMB held on
//! it), the dev drain heats the core, the brake (empty hands, LMB held) pulls the rod with the most
//! life into the hand and burns it once, G drops it. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn rods_scenario_passes_headless() {
    let o = Options { scenario: Some("rods".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("rods-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
