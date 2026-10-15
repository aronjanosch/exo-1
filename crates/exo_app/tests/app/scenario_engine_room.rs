use crate::common;
use exo_app::Options;

#[test]
fn engine_room_scenario_passes_headless() {
    let o = Options { scenario: Some("engine_room".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("engine-room-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
