//! The engine room at the dev area: the corridor and its wall, the door opened and shut with
//! F, the room's air low and back (the health shows it), the alarm cycled to Warn and Danger.
//! Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn engine_room_scenario_passes_headless() {
    let o = Options { scenario: Some("engine_room".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("engine-room-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
