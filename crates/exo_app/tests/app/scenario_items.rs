//! C1: items at the dev area's item table: collect, hold, drop, throw, a lost race for an item,
//! the inventory drag, and a wrench riding the cabin through a take-off. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn items_scenario_passes_headless() {
    let o = Options { scenario: Some("items".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("items-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
