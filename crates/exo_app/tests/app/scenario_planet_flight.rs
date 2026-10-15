//! Not a timing check: frame times are reported only.
use crate::common;
use exo_app::{build_app, scenario::Script, Options};

#[test]
#[ignore]
fn planet_flight_at_100_km_reports_frame_spikes() {
    let o = Options {
        scenario: Some("planet-flight".into()),
        headless: true,
        realtime: true,
        radius: Some(100_000.0),
        atmosphere: Some(11_670.0),
        out_dir: common::out_dir("planet-flight"),
        ..Default::default()
    };
    let mut app = build_app(&o);
    app.finish();
    app.cleanup();
    let t0 = std::time::Instant::now();
    let tick = exo_app::TICK;
    let mut next = std::time::Instant::now();
    while !app.world().resource::<Script>().done {
        app.update();
        next += tick;
        let now = std::time::Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
        assert!(t0.elapsed().as_secs() < 600, "scenario timed out");
    }
    let failures = app.world().resource::<Script>().ctx.failures;
    println!("planet-flight: {failures} failures, {:.1} s wall, out {}", t0.elapsed().as_secs_f64(), o.out_dir.display());
    assert_eq!(failures, 0);
}
