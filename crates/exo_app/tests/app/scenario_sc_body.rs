use crate::common;
use exo_app::Options;

#[test]
fn scenario_sc_body_passes_headless() {
    let o = Options { scenario: Some("sc-body".into()), headless: true, out_dir: common::out_dir("sc-body-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
