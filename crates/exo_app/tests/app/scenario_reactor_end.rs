use crate::common;
use exo_app::Options;

#[test]
fn reactor_end_scenario_passes_headless() {
    let o = Options { scenario: Some("reactor_end".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("reactor-end-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
