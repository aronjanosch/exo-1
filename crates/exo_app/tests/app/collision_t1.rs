//! Spike 8 T1 in Bevy: the heightfield patches Avian collides with agree with
//! planet_core's height function (rays at every interior patch sample, own patch only;
//! the parked ship stands in the way of rays from above).
use avian3d::prelude::*;
use bevy::ecs::system::SystemState;
use bevy::math::DVec3;
use bevy::prelude::*;
use exo_app::{build_app, env::PlanetRes, ring::Ring, Options};

#[test]
fn collision_patches_match_height_function() {
    let mut app = build_app(&Options { headless: true, ..Default::default() });
    app.finish();
    app.cleanup();
    let t0 = std::time::Instant::now();
    loop {
        app.update();
        let r = app.world().resource::<Ring>();
        if r.pending() == 0 && r.patches.len() > 100 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert!(t0.elapsed().as_secs() < 60);
    }
    for _ in 0..3 {
        app.update(); // let Avian add the last patches to its trees
    }
    let planet = app.world().resource::<PlanetRes>().clone();
    let patches: Vec<(Entity, DVec3, bevy::math::DQuat)> = app.world().resource::<Ring>().patches.values().map(|(e, c, r)| (*e, *c, *r)).collect();
    let mut state: SystemState<SpatialQuery> = SystemState::new(app.world_mut());
    let sq = state.get(app.world()).unwrap();
    let (mut worst, mut n, mut misses) = (0.0f64, 0u32, 0u32);
    for (e, c, r) in &patches {
        let up = *r * DVec3::Y;
        for j in 1..31 {
            for i in 1..31 {
                let (x, z) = (i as f64 - 15.5, j as f64 - 15.5);
                let from = *c + *r * DVec3::new(x, 50.0, z);
                let dir = Dir3::new((-up).as_vec3()).unwrap();
                let Some(hit) = sq.cast_ray_predicate(from, dir, 200.0, true, &SpatialQueryFilter::default(), &|x| x == *e) else {
                    misses += 1;
                    continue;
                };
                let p = from - up * hit.distance;
                let err = ((p - planet.centre).length() - planet.surface(p - planet.centre)).abs();
                worst = worst.max(err);
                n += 1;
            }
        }
    }
    println!("T1 collision: {} patches, {n} samples, worst |collision - height_at| = {:.3} mm, misses {misses}", patches.len(), worst * 1000.0);
    // A ray exactly through a heightfield vertex can slip between triangles (parry); allow a few.
    assert!(misses < 10 && worst < 0.001);
}
