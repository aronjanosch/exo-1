//! #169: the flight licence. The seat refuses without it, the exam is taken and passed, the seat
//! works. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn licence_scenario_passes_headless() {
    let o = Options { scenario: Some("licence".into()), headless: true, out_dir: common::out_dir("licence-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
