//! A remote ship through the real snapshot path: the walker in its cabin and beside it, also at
//! warp speed (#16).
use crate::common;
use exo_app::Options;

#[test]
fn foreign_scenario_passes_headless() {
    let o = Options { scenario: Some("foreign".into()), headless: true, out_dir: common::out_dir("foreign-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
