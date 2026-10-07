//! Replay matrix of spike 4 without sockets, on a synthetic path. The recorded path of the full
//! scenario runs in `exo_app/tests/scenario_full.rs`.
use net_core::replay::{matrix_cases, run_matrix, synthetic_path};

const PASS_BOUND_MM: f64 = 10.0;
/// Spike 4 asserted exactly 0 underruns for its seeds; another RNG gives a rare hold (four or
/// more lost snapshots in a row at 10 % loss), so the bound here is small instead of zero.
const HOLD_BOUND_PERCENT: f64 = 0.05;

#[test]
fn matrix_has_96_cases() {
    assert_eq!(matrix_cases().len(), 96);
}

#[test]
fn synthetic_path_matrix_passes_at_30hz_150ms() {
    let tr = synthetic_path();
    let results = run_matrix(&tr);
    let mut worst: f64 = 0.0;
    for r in results.iter().filter(|r| r.case.rate == 30 && r.case.buffer_ms == 150) {
        assert!(r.all.hold_percent < HOLD_BOUND_PERCENT, "{:?}", r.case);
        worst = worst.max(r.all.error_p95_mm);
    }
    assert!(worst < PASS_BOUND_MM, "worst p95 {worst} mm");
}
