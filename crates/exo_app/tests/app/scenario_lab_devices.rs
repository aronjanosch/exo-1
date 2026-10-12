//! #287: the lab devices on the dev area: shelf, mixer, heater, centrifuge, filler, dummy; a good and a burnt run. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn lab_devices_scenario_passes_headless() {
    let o = Options { scenario: Some("lab_devices".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("lab-devices-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
