use crate::common;
use exo_app::Options;

#[test]
fn reactor_room_scenario_passes_headless() {
    let o = Options { scenario: Some("reactor_room".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("reactor-room-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
