//! Night extra E2: crates stack, the walker stops at crates and stands on them. Without a window.
mod common;
use exo_app::Options;

#[test]
fn crate_stack_scenario_passes_headless() {
    let o = Options { scenario: Some("crate-stack".into()), headless: true, out_dir: std::env::temp_dir().join("exo-crate-stack-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
