//! The dev area and the test console: walk to it, use it with F, click, leave with F and
//! Escape. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn dev_area_scenario_passes_headless() {
    let o = Options { scenario: Some("dev_area".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("dev-area-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
