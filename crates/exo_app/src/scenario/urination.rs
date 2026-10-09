//! Bladder cycle and ballistic droplets through the shared scripted Controls path.
use super::*;

pub(super) fn urination_steps(out_dir: &std::path::Path, windowed: bool) -> Vec<Step> {
    let dir = out_dir.to_path_buf();
    let mut started = None;
    let mut samples = 0;
    let mut max_aim_error: f64 = 0.0;
    let mut max_horizontal: f64 = 0.0;
    let mut max_vertical: f64 = 0.0;
    let mut max_launch_speed_error: f64 = 0.0;
    let mut previous_fullness = 1.0;
    let mut decreasing = true;
    let mut photographed = false;
    vec![
        Box::new(|w, c| {
            begin(w, c, "automatic urination");
            let e = w.query_filtered::<Entity, With<Player>>().single(w).unwrap();
            w.get_mut::<crate::urination::Bladder>(e).unwrap().0 = Default::default();
            c.v.insert("emitted0", w.resource::<crate::urination::UrineParticles>().emitted as f64);
            c.v.insert("hits0", w.resource::<crate::urination::UrineParticles>().hits as f64);
            let p = player_world(w);
            let ship = ship_frame_of(w).origin;
            face_towards(w, p + (p - ship));
            true
        }),
        wait(30.0),
        Box::new(|w, c| {
            let b = w.query::<&crate::urination::Bladder>().single(w).unwrap().0;
            check(c, !b.urinating() && (b.fullness() - 0.5).abs() < 0.005, "bladder half full after 30 seconds, no jet".into());
            check(c, w.resource::<crate::urination::UrineJet>().0.is_none(), "no urine jet while filling".into());
            true
        }),
        wait(29.8),
        Box::new(|w, c| {
            let b = w.query::<&crate::urination::Bladder>().single(w).unwrap().0;
            check(c, !b.urinating() && b.fullness() > 0.99, "no early urination just before 60 seconds".into());
            true
        }),
        Box::new(move |w, c| {
            let b = w.query::<&crate::urination::Bladder>().single(w).unwrap().0;
            if b.urinating() {
                let start = *started.get_or_insert_with(|| {
                    c.p.insert("urination_start", player_world(w));
                    c.t
                });
                samples += 1;
                decreasing &= b.fullness() <= previous_fullness;
                previous_fullness = b.fullness();
                let f = ship_frame_of(w);
                let planet = planet(w);
                let (feet, up, expected) = with_player(w, |p| (p.world_pos(f), p.world_up(f), p.world_look(f)));
                match w.resource::<crate::urination::UrineJet>().0 {
                    Some(jet) => {
                        max_aim_error = max_aim_error.max(jet.direction.distance(expected));
                        max_aim_error = max_aim_error.max(jet.start.distance(feet + up * 0.9 + expected * 0.4));
                        let carrier = with_player(w, |p| p.w.vel);
                        let right = expected.cross(up).normalize();
                        let vertical_up = right.cross(expected).normalize();
                        // Undo the fractional gravity step of the last tick's births.
                        for p in w.query::<&crate::urination::UrineParticle>().iter(w).filter(|p| p.0.age < c.dt && p.1.is_none()) {
                            let launch = p.0.velocity - carrier - planet.gravity_at(p.0.previous) * p.0.age;
                            max_launch_speed_error = max_launch_speed_error.max((launch.length() - walker_core::urine::SPEED).abs());
                            max_horizontal = max_horizontal.max(launch.dot(right).atan2(launch.dot(expected)).to_degrees().abs());
                            max_vertical = max_vertical.max(launch.dot(vertical_up).atan2(launch.dot(expected)).to_degrees().abs());
                        }
                    }
                    None => max_aim_error = f64::INFINITY,
                }
                keys(w, &[KeyCode::KeyW], true);
                w.resource_mut::<Controls>().mouse += Vec2::new(2.0, if c.t - start < 2.5 { -1.0 } else { 1.0 });
                if !photographed && c.t - start > 2.0 {
                    shot(w, c, &dir, windowed, "urinating");
                    photographed = true;
                }
            } else if let Some(start) = started {
                keys(w, &[KeyCode::KeyW], false);
                let duration = c.t - start;
                let moved = player_world(w).distance(c.p["urination_start"]);
                check(c, (duration - 5.0).abs() < 0.04 && samples >= 299, format!("automatic urination lasts 5 seconds ({duration:.3} s)"));
                check(c, decreasing && b.fullness() < 0.002, "bladder drains continuously to empty".into());
                check(c, max_aim_error < 1e-8, format!("hip jet follows yaw, pitch and position (max error {max_aim_error:.2e})"));
                check(c, max_horizontal > 0.1 && max_horizontal <= 0.5 + 1e-8 && max_vertical > 0.1 && max_vertical <= 0.5 + 1e-8,
                    format!("random launch spread within +/-0.5 deg per axis (horizontal {max_horizontal:.4}, vertical {max_vertical:.4} deg)"));
                check(c, max_launch_speed_error < 1e-8, format!("spread preserves 8 m/s relative launch speed (max error {max_launch_speed_error:.2e})"));
                check(c, moved > 5.0, format!("movement remains possible while urinating ({moved:.1} m)"));
                check(c, w.resource::<crate::urination::UrineJet>().0.is_none(), "jet stops when bladder is empty".into());
                let count = w.resource::<crate::urination::UrineParticles>().emitted as f64 - c.v["emitted0"];
                check(c, count == 600.0, format!("120 droplets per second for 5 seconds ({count:.0} births)"));
                let alive = w.query::<&crate::urination::UrineParticle>().iter(w).count();
                check(c, alive > 0 && alive < 600, format!("independent droplets remain in flight after emission stops ({alive} alive)"));
                return true;
            }
            if c.t > 6.0 {
                keys(w, &[KeyCode::KeyW], false);
                check(c, false, "automatic urination timed out".into());
                return true;
            }
            false
        }),
        wait(30.0),
        Box::new(|w, c| {
            let b = w.query::<&crate::urination::Bladder>().single(w).unwrap().0;
            check(c, !b.urinating() && (b.fullness() - 0.5).abs() < 0.005, "bladder starts filling again after urination".into());
            let hits = w.resource::<crate::urination::UrineParticles>().hits as f64 - c.v["hits0"];
            check(c, hits > 0.0 && w.query::<&crate::urination::UrineParticle>().iter(w).count() == 0, format!("droplets disappear on impact, no leftovers ({hits:.0} hits)"));
            end(w, c, "60-second fill, 5-second drain, aim and movement checked".into());
            true
        }),
    ]
}

#[derive(Resource)]
struct ParticleProbes {
    ground: Entity,
    wall: Entity,
    vacuum: Entity,
    vacuum_start: DVec3,
    expired0: u64,
}

/// Fixture births test the same particle system as the Controls-driven emitter.
pub(super) fn particle_environment_checks() -> Vec<Step> {
    vec![
        Box::new(|w, c| {
            begin(w, c, "droplet collisions and vacuum lifetime");
            let planet = planet(w);
            let feet = player_world(w);
            let up = planet.up(feet);
            let frame = ship_frame_of(w);
            let spawn = |w: &mut World, position, velocity| {
                w.spawn(crate::urination::UrineParticle(walker_core::urine::Particle { previous: position, position, velocity, age: 0.0 }, None)).id()
            };
            let ground = spawn(w, feet + up * 0.1, -up * 8.0);
            let wall = spawn(w, frame.to_world(DVec3::new(1.9, 1.6, 0.0)), frame.rot * DVec3::X * 8.0);
            let vacuum_start = planet.centre + DVec3::Y * (planet.radius + 20_000.0);
            let vacuum = spawn(w, vacuum_start, DVec3::X * 8.0);
            let expired0 = w.resource::<crate::urination::UrineParticles>().expired;
            w.insert_resource(ParticleProbes { ground, wall, vacuum, vacuum_start, expired0 });
            true
        }),
        wait(1.0),
        Box::new(|w, c| {
            let probes = w.resource::<ParticleProbes>();
            check(c, w.get::<crate::urination::UrineParticle>(probes.ground).is_none(), "droplet despawns on terrain impact".into());
            check(c, w.get::<crate::urination::UrineParticle>(probes.wall).is_none(), "droplet despawns on a thin ship wall".into());
            let p = w.get::<crate::urination::UrineParticle>(probes.vacuum).map(|p| p.0);
            check(c, p.is_some_and(|p| p.velocity == DVec3::X * 8.0 && p.position.distance(probes.vacuum_start + DVec3::X * (8.0 * p.age)) < 1e-8), "without gravity droplets keep a straight trajectory and constant speed".into());
            true
        }),
        wait(8.8),
        Box::new(|w, c| {
            let e = w.resource::<ParticleProbes>().vacuum;
            check(c, w.get::<crate::urination::UrineParticle>(e).is_some_and(|p| p.0.age > 9.8 && p.0.age < 10.0), "vacuum droplet remains alive just before 10 seconds".into());
            true
        }),
        wait(0.3),
        Box::new(|w, c| {
            let probes = w.resource::<ParticleProbes>();
            check(c, w.get::<crate::urination::UrineParticle>(probes.vacuum).is_none() && w.resource::<crate::urination::UrineParticles>().expired == probes.expired0 + 1, "vacuum droplet despawns at its 10-second lifetime".into());
            w.remove_resource::<ParticleProbes>();
            end(w, c, "terrain, ship wall, zero gravity and lifetime checked".into());
            true
        }),
    ]
}

#[derive(Resource, Clone, Copy)]
struct CabinProbes {
    ship: Entity,
    gravity: Entity,
    weightless: Option<Entity>,
    leaving: Option<Entity>,
    start: DVec3,
}

pub(super) fn cabin_particle_checks() -> Vec<Step> {
    vec![
        Box::new(|w, c| {
            begin(w, c, "droplets in a moving tilted cabin, LAG on and off");
            let planet = planet(w);
            let start = planet.centre + DVec3::Y * (planet.radius + 20_000.0);
            let mut commands = w.commands();
            let ship = crate::net_live::spawn_proxy(&mut commands, 2, start, DQuat::from_rotation_z(1.1));
            w.flush();
            w.get_mut::<LinearVelocity>(ship).unwrap().0 = DVec3::X * 350.0;
            w.insert_resource(CabinProbes { ship, gravity: Entity::PLACEHOLDER, weightless: None, leaving: None, start });
            true
        }),
        // Avian initializes the new proxy's child collider poses on the next physics step.
        wait(0.1),
        Box::new(|w, _| {
            let ship = w.resource::<CabinProbes>().ship;
            let local = DVec3::new(0.0, 1.5, 1.5);
            let gravity = w.spawn(crate::urination::UrineParticle(walker_core::urine::Particle { previous: local, position: local, velocity: DVec3::NEG_Z * 8.0, age: 0.0 }, Some(ship))).id();
            w.resource_mut::<CabinProbes>().gravity = gravity;
            true
        }),
        wait(0.2),
        Box::new(|w, c| {
            let probes = *w.resource::<CabinProbes>();
            let p = w.get::<crate::urination::UrineParticle>(probes.gravity);
            check(c, p.is_some_and(|p| p.1 == Some(probes.ship)
                && (p.0.velocity.y + 9.81 * p.0.age).abs() < 1e-8
                && (p.0.position.y - (1.5 - 4.905 * p.0.age * p.0.age)).abs() < 1e-8),
                "cabin-local droplets follow the tilted LAG field while carried by the frame".into());
            check(c, w.get::<Position>(probes.ship).unwrap().0.distance(probes.start) > 50.0, "droplet cabin moved at 350 m/s".into());
            w.get_mut::<RemoteShip>(probes.ship).unwrap().lag = 0.0;
            let local = DVec3::new(0.0, 1.5, 1.5);
            let weightless = w.spawn(crate::urination::UrineParticle(walker_core::urine::Particle { previous: local, position: local, velocity: DVec3::NEG_Z * 8.0, age: 0.0 }, Some(probes.ship))).id();
            w.resource_mut::<CabinProbes>().weightless = Some(weightless);
            true
        }),
        wait(0.2),
        Box::new(|w, c| {
            let probes = *w.resource::<CabinProbes>();
            let p = w.get::<crate::urination::UrineParticle>(probes.weightless.unwrap());
            check(c, p.is_some_and(|p| p.1 == Some(probes.ship) && p.0.velocity.distance(DVec3::NEG_Z * 8.0) < 1e-8
                && p.0.position.distance(DVec3::new(0.0, 1.5, 1.5 - 8.0 * p.0.age)) < 1e-8),
                "LAG off in space: cabin droplets fly straight, still carried by the frame".into());
            let local = DVec3::new(0.0, 1.5, 3.95);
            let leaving = w.spawn(crate::urination::UrineParticle(walker_core::urine::Particle { previous: local, position: local, velocity: DVec3::Z * 8.0, age: 0.0 }, Some(probes.ship))).id();
            w.resource_mut::<CabinProbes>().leaving = Some(leaving);
            true
        }),
        wait(0.05),
        Box::new(|w, c| {
            let probes = *w.resource::<CabinProbes>();
            let p = w.get::<crate::urination::UrineParticle>(probes.leaving.unwrap());
            check(c, p.is_some_and(|p| p.1.is_none() && p.0.velocity.distance(DVec3::X * 350.0 + DVec3::Z * 8.0) < 1e-8),
                "leaving the cabin hands the frame velocity to the droplet".into());
            for e in [probes.gravity, probes.weightless.unwrap(), probes.leaving.unwrap(), probes.ship] { w.despawn(e); }
            w.remove_resource::<CabinProbes>();
            end(w, c, "moving frame and switched LAG checked".into());
            true
        }),
    ]
}
