//! #288 phase 3: the dev area as one crew loop: reactor, fire, the captain's displays and orders,
//! the bot's real work. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn dev_area_loop_scenario_passes_headless() {
    let o = Options { scenario: Some("dev_area_loop".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("dev-area-loop-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
