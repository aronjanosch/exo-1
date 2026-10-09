//! #48: a fast-forwarded day on every planet; sun direction and brightness as the core says.
mod common;
use exo_app::Options;

#[test]
fn daynight_passes_headless() {
    let o = Options { scenario: Some("daynight".into()), headless: true, out_dir: std::env::temp_dir().join("exo-daynight"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
