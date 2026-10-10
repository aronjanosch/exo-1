#![allow(dead_code)] // perf.rs uses a part of this
//! Runs a scripted scenario headless until it is done and checks it passed.
use exo_app::{build_app, scenario::Script, walker::WalkStats, Options};

pub fn run_scenario(o: &Options) -> bevy::app::App {
    let mut app = build_app(o);
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
    println!("{:?} scenario: {failures} failures, {rescues} rescues, {:.1} s wall", o.scenario, t0.elapsed().as_secs_f64());
    assert_eq!((failures, rescues), (0, 0));
    app
}

/// Output folder for one test run (#117): inside the lane's own target dir, unique per process,
/// created fresh. Nothing fixed in /tmp, so lanes running the same test at once do not collide.
pub fn out_dir(name: &str) -> std::path::PathBuf {
    let d = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("exo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
