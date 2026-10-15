//! The first `update()` runs no fixed tick: `Time<Real>` only records the instant.
use bevy::prelude::*;
use exo_app::{build_app, Options};

#[derive(Resource, Default)]
struct FixedRuns(u32);

#[test]
fn first_update_runs_no_fixed_tick_then_one_per_update() {
    let mut app = build_app(&Options { headless: true, ..Default::default() });
    app.init_resource::<FixedRuns>();
    app.add_systems(FixedUpdate, |mut n: ResMut<FixedRuns>| n.0 += 1);
    app.finish();
    app.cleanup();
    let mut runs = Vec::new();
    for _ in 0..5 {
        app.update();
        runs.push(app.world().resource::<FixedRuns>().0);
    }
    assert_eq!(runs, [0, 1, 2, 3, 4]);
}
