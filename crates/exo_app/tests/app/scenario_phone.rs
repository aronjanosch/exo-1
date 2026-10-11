//! Round 6: the phone and its six tabs. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn phone_scenario_passes_headless() {
    let o = Options { scenario: Some("phone".into()), headless: true, out_dir: common::out_dir("phone-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
