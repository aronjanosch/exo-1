//! #16: a remote ship at warp speed next to the walking walker leaves the walk unchanged.
mod common;
use exo_app::Options;

#[test]
fn foreign_warp_scenario_passes_headless() {
    let o = Options { scenario: Some("foreign_warp".into()), headless: true, out_dir: std::env::temp_dir().join("exo-foreign-warp-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
