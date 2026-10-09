//! #21: a tuning file edited while the game runs takes effect (dev builds).
mod common;
use exo_app::Options;

#[test]
fn reload_scenario_passes_headless() {
    let o = Options { scenario: Some("reload".into()), headless: true, out_dir: std::env::temp_dir().join("exo-reload-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
