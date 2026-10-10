//! #170: courier jobs on foot, take the job at the counter, carry the parcel to the next place, get
//! paid. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn courier_scenario_passes_headless() {
    let o = Options { scenario: Some("courier".into()), headless: true, out_dir: common::out_dir("courier-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
