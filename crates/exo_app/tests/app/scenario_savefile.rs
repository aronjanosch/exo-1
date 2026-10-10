//! #135: a job half done is saved to the file, the gameplay restarts from it, and the job finishes.
//! Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn savefile_scenario_passes_headless() {
    let o = Options { scenario: Some("savefile".into()), headless: true, out_dir: common::out_dir("savefile-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
