//! The reactor in the engine room: Auto runs calm; the dev faults screen's Pump break goes
//! Warn then Danger; Reset reactor brings it back; Load drop on Auto overshoots above the optimal
//! band; the readout's MAN and fission clicks bring it back. Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn reactor_scenario_passes_headless() {
    let o = Options { scenario: Some("reactor".into()), start: Some("dev_area".into()), headless: true, out_dir: common::out_dir("reactor-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
