use crate::common;
use exo_app::Options;

#[test]
fn sniffer_scenario_passes_headless() {
    let o = Options { scenario: Some("sniffer".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("sniffer-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
