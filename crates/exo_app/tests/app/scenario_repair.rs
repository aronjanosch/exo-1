//! Repair at the machine. The broken coolant pump makes no progress before three hits in
//! the timing ring, then rises; the screwdriver does nothing at the pump; the junction box at 70 %
//! has no ring and a slow repair that F ends and keeps; full ends the session. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn repair_scenario_passes_headless() {
    let o = Options { scenario: Some("repair".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("repair-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
