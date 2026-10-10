//! #13: how many fixed ticks each `update()` of the headless app runs.
//!
//! bevy_time 0.19.1, `real.rs` `Time<Real>::update_with_instant`: the first call only records
//! the instant (`first_update`, `last_update`) and leaves the delta at zero. Under
//! `TimeUpdateStrategy::ManualDuration` the first `update()` therefore advances time by 0 and
//! runs no `FixedUpdate`; every later `update()` advances exactly one `TICK` and runs one.
//!
//! Nothing in the scenarios counts updates: the script (`Ctx::ticks`), the walker statistics and
//! the `--record` path all count fixed steps, which start with the second update. So the scenario
//! numbers do not depend on it and no warm-up update is needed.
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
