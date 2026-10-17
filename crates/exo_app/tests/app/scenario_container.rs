use crate::common;
use exo_app::Options;

#[test]
fn container_scenario_passes_headless() {
    let o = Options { scenario: Some("container".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("container-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
