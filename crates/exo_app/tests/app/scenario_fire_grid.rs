//! #300: the fire grid: the faults screen's switch to the cells, a fire at the reactor spreads
//! cell to cell within 30 s while the oxygen falls, the corridor catches with the door open, and
//! the extinguisher puts the reactor's front cell out. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn fire_grid_scenario_passes_headless() {
    let o = Options { scenario: Some("fire_grid".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("fire-grid-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
