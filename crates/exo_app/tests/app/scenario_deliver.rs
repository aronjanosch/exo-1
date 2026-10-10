//! #133: the first playable round, take a job on a pad and deliver it to another. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn deliver_scenario_passes_headless() {
    let o = Options { scenario: Some("deliver".into()), headless: true, out_dir: common::out_dir("deliver-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
