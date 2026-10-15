use crate::common;
use exo_app::Options;

#[test]
fn blast_scenario_passes_headless() {
    let o = Options { scenario: Some("blast".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("blast-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
