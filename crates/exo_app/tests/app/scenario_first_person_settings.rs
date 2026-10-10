use crate::common;
use exo_app::Options;

#[test]
fn first_person_settings_scenario_passes_headless() {
    let o = Options { scenario: Some("first-person-settings".into()), headless: true, out_dir: common::out_dir("first-person-settings"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
