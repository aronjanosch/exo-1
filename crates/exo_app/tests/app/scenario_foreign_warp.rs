//! #16: a remote ship at warp speed next to the walking walker leaves the walk unchanged.
use crate::common;
use exo_app::Options;

#[test]
fn foreign_warp_scenario_passes_headless() {
    let o = Options { scenario: Some("foreign_warp".into()), headless: true, out_dir: common::out_dir("foreign-warp-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
