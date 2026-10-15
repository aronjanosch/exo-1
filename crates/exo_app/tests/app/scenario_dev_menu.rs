use crate::common;
use exo_app::Options;

#[test]
fn dev_menu_scenario_passes_headless() {
    let o = Options { scenario: Some("dev-menu".into()), headless: true, out_dir: common::out_dir("dev-menu-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
