//! Display-only extrapolation during an underrun, on the recorded path. Off by default (spike 4
//! rule: hold); this measures what it would buy.
use net_core::replay::{matrix_cases, run_case_extrapolated, Trajectory};

#[test]
fn extrapolation_against_hold() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../results/full-path.bin");
    let tr = Trajectory::parse(&std::fs::read(path).unwrap()).unwrap();
    let mut out = String::from("[\n");
    let mut rows = Vec::new();
    for case in matrix_cases().into_iter().filter(|c| ((c.rate == 30 && c.buffer_ms == 150) || (c.rate == 20 && c.buffer_ms == 100 && c.delay_ms == 150)) && c.loss_percent >= 5) {
        for ms in [0.0, 50.0, 100.0, 200.0] {
            let r = run_case_extrapolated(&tr, case, ms / 1000.0);
            println!(
                "{} players {} Hz {} ms delay {} % loss, extrapolate {:>3} ms: p95 {:.3} mm, max {:.1} mm, holds {:.4} %",
                case.players, case.rate, case.delay_ms, case.loss_percent, ms, r.all.error_p95_mm, r.all.error_max_mm, r.all.hold_percent
            );
            rows.push((case, ms, r));
        }
    }
    for (i, (c, ms, r)) in rows.iter().enumerate() {
        out += &format!(
            "  {{\"players\":{},\"rate\":{},\"delay_ms\":{},\"loss_percent\":{},\"extrapolate_ms\":{},\"error_p95_mm\":{:.4},\"error_max_mm\":{:.3},\"hold_percent\":{:.4}}}{}\n",
            c.players, c.rate, c.delay_ms, c.loss_percent, ms, r.all.error_p95_mm, r.all.error_max_mm, r.all.hold_percent, if i + 1 < rows.len() { "," } else { "" }
        );
    }
    out += "]\n";
    if let Ok(p) = std::env::var("EXTRAP_OUT") {
        std::fs::write(p, out).unwrap();
    }
    // Extrapolation must never make the worst case worse than holding.
    for chunk in rows.chunks(4) {
        assert!(chunk[2].2.all.error_max_mm <= chunk[0].2.all.error_max_mm + 1e-9, "100 ms extrapolation worse than hold: {:?}", chunk[0].0);
    }
}
