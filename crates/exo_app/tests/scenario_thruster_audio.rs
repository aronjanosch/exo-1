//! #150: the thruster sound layers follow strafe, boost and rest, driven through `Controls`.
mod common;
use exo_app::Options;

#[test]
fn thruster_audio_scenario_passes_headless() {
    let o = Options { scenario: Some("thruster-audio".into()), headless: true, out_dir: common::out_dir("thruster-audio-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
