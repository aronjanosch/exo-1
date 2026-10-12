//! #288: the captain's displays in the dev area: overview, missions, ship state, starmap, bot
//! priorities, through the pointer. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn captain_displays_scenario_passes_headless() {
    let o = Options { scenario: Some("captain_displays".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("captain-displays-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
