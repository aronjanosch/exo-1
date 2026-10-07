//! The full scripted run (walk, ramp, space and back, landing, cabin at speed) without a window.
//! The run records the ship and walker path; the 96 network cases of spike 4 then replay on it.
use exo_app::{build_app, record::Recorder, scenario::Script, walker::WalkStats, Options};
use net_core::replay::{matrix_cases, run_case_extrapolated, run_matrix};

/// Spike 4 acceptance at 30 Hz with a 150 ms buffer: p95 position error under 10 mm.
const PASS_BOUND_MM: f64 = 10.0;
/// Spike 4 asserted exactly 0 underruns for its seeds; another RNG gives a rare hold (four or
/// more lost snapshots in a row at 10 % loss), so the bound is small instead of zero.
const HOLD_BOUND_PERCENT: f64 = 0.05;

#[test]
fn full_scenario_passes_headless_and_network_replays_on_its_path() {
    let out = std::env::temp_dir().join("exo-full-scenario");
    let o = Options { scenario: Some("full".into()), headless: true, out_dir: out.clone(), record: Some(out.join("full-path.bin")), ..Default::default() };
    let mut app = build_app(&o);
    app.finish();
    app.cleanup();
    let t0 = std::time::Instant::now();
    while !app.world().resource::<Script>().done {
        app.update();
        // The ring builds patches on worker threads in wall time; give them a moment.
        std::thread::sleep(std::time::Duration::from_micros(200));
        assert!(t0.elapsed().as_secs() < 600, "scenario timed out");
    }
    let failures = app.world().resource::<Script>().ctx.failures;
    let rescues = app.world().resource::<WalkStats>().rescues;
    println!("full scenario: {failures} failures, {rescues} rescues, {:.1} s wall", t0.elapsed().as_secs_f64());
    assert_eq!((failures, rescues), (0, 0));

    let tr = &app.world().resource::<Recorder>().traj;
    let results = run_matrix(tr);
    assert_eq!(results.len(), 96);
    for r in results.iter().filter(|r| r.case.rate == 30 && r.case.buffer_ms == 150) {
        assert!(r.all.hold_percent < HOLD_BOUND_PERCENT, "underruns in {:?}: {} %", r.case, r.all.hold_percent);
        assert!(r.all.error_p95_mm < PASS_BOUND_MM, "{:?}: p95 {} mm", r.case, r.all.error_p95_mm);
    }
    // Display-only extrapolation (100 ms, the default) must never make the worst case worse than holding.
    for case in matrix_cases().into_iter().filter(|c| c.rate == 30 && c.buffer_ms == 150 && c.loss_percent >= 5) {
        let hold = run_case_extrapolated(tr, case, 0.0).all.error_max_mm;
        let extra = run_case_extrapolated(tr, case, 0.1).all.error_max_mm;
        assert!(extra <= hold + 1e-9, "100 ms extrapolation worse than hold: {case:?}");
    }
}
