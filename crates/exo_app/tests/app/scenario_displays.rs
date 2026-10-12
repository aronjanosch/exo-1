//! #285: the pointer on a display through the app: hover, press and release elsewhere, a disabled
//! button, a click off the panel, the slider. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn displays_scenario_passes_headless() {
    let o = Options { scenario: Some("displays".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("displays-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
