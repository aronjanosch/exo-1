//! #286: the engineer's corner of the dev area: reactor on Auto, a bad manual setting, fire, scram,
//! repair, extinguisher, door, restart. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn reactor_scenario_passes_headless() {
    let o = Options { scenario: Some("reactor".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("reactor-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
