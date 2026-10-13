//! #295: health at the dev area: the hot plate burns, a bandage on oneself, the glowing barrel and
//! the no-air booth take the walker down, the respawn on the pad after five seconds. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn health_scenario_passes_headless() {
    let o = Options { scenario: Some("health".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("health-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
