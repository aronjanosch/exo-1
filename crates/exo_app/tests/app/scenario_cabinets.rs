//! #287: the dev area's storage: refusals, the key, moving stacks, nothing lost. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn cabinets_scenario_passes_headless() {
    let o = Options { scenario: Some("cabinets".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("cabinets-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
