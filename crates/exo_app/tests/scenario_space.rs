//! Issue #5: stepping out of the ship in space, without a window.
use exo_app::{build_app, scenario::Script, walker::WalkStats, Options};

#[test]
fn space_scenario_passes_headless() {
    let out = std::env::temp_dir().join("exo-space-scenario");
    let o = Options { scenario: Some("space".into()), headless: true, out_dir: out, ..Default::default() };
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
    assert_eq!((failures, rescues), (0, 0));
}
