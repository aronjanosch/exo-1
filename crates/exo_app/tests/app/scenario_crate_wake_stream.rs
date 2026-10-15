use crate::common;
use exo_app::Options;

#[test]
fn crate_wake_stream_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-wake-stream".into()), headless: true, out_dir: common::out_dir("crate-wake-stream-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
