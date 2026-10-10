//! Round 5 (#198): the SC body through the controls: the spool and jerk of the forward thrust, the
//! boost ramp, from a hover at 400 m.
use crate::common;
use exo_app::Options;

#[test]
fn scenario_sc_body_passes_headless() {
    let o = Options { scenario: Some("sc-body".into()), headless: true, out_dir: common::out_dir("sc-body-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
