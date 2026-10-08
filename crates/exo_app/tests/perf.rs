//! #19: `--perf` writes the JSON, a run against its own baseline passes, a slowed step fails.
use exo_app::{build_app, perf::PerfOptions, scenario::Script, Options};

fn run(perf: PerfOptions, out: &std::path::Path) -> u32 {
    let o = Options { scenario: Some("walk".into()), headless: true, out_dir: out.to_path_buf(), perf: Some(perf), ..Default::default() };
    let mut app = build_app(&o);
    app.finish();
    app.cleanup();
    while !app.world().resource::<Script>().done {
        app.update();
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
    app.world().resource::<Script>().ctx.failures
}

#[test]
fn baseline_passes_and_slowed_step_fails() {
    let out = std::env::temp_dir().join("exo-perf-test");
    let _ = std::fs::remove_dir_all(&out);
    // Generous tolerance: other test binaries may share the machine.
    let opt = PerfOptions { baseline: out.join("baseline.json"), tolerance: 1.0, ..Default::default() };
    assert_eq!(run(PerfOptions { save_baseline: true, ..opt.clone() }, &out), 0);
    let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out.join("perf-walk.json")).unwrap()).unwrap();
    let phases = report["phases"].as_array().unwrap();
    assert!(phases.iter().any(|p| p["name"] == "walk 20 s (run)" && p["step_ms"]["p95"].as_f64().is_some()), "{report}");
    assert!(phases.iter().all(|p| p["frame_ms"].is_null()), "headless has no frame times");
    assert_eq!(run(opt.clone(), &out), 0, "a run against its own baseline passes");
    assert!(run(PerfOptions { slow_ms: 10.0, ..opt }, &out) > 0, "a step slowed by 10 ms fails");
}
