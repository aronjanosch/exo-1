mod sc_common;
use flight_core::sc::{ModeCmds, ScShip};
use flight_core::{BodyState, FlightInput, PlanetEnv};
use glam::{DQuat, DVec2, DVec3};
use sc_common::*;

fn advance(
    ship: &mut ScShip,
    body: &mut BodyState,
    input: &FlightInput,
    cmds: &ModeCmds,
    env: &impl PlanetEnv,
) {
    let out = ship.step(body, input, cmds, env, DT);
    body.lin_vel = out.lin_vel;
    body.ang_vel = out.ang_vel;
    body.integrate(DT);
}

#[test]
fn fully_decoupled_mouse_rotation_preserves_the_world_velocity_in_vacuum() {
    let mut ship = ScShip::new(tuning());
    ship.modes.coupled = false;
    ship.modes.coupling = 0.0;
    let velocity = DVec3::new(40.0, 20.0, -150.0);
    let mut body = BodyState {
        lin_vel: velocity,
        ..body_at(DVec3::Y * 10000.0)
    };
    let input = FlightInput {
        mouse: DVec2::new(0.006, -0.004),
        roll: 0.4,
        piloted: true,
        ..Default::default()
    };
    for _ in 0..600 {
        advance(
            &mut ship,
            &mut body,
            &input,
            &ModeCmds::default(),
            &Space::default(),
        );
        assert!(
            (body.lin_vel - velocity).length() < 1e-8,
            "rotation changed velocity: {:?}",
            body.lin_vel
        );
    }
    assert!(
        (body.rot * DVec3::NEG_Z)
            .angle_between(DVec3::NEG_Z)
            .to_degrees()
            > 30.0
    );
}

#[test]
fn selecting_decoupled_does_not_brake_away_the_existing_velocity() {
    let mut ship = ScShip::new(tuning());
    let velocity = DVec3::NEG_Z * 150.0;
    let mut body = BodyState {
        lin_vel: velocity,
        ..body_at(DVec3::Y * 10000.0)
    };
    let input = FlightInput {
        piloted: true,
        ..Default::default()
    };
    for tick in 0..300 {
        let cmds = ModeCmds {
            decoupled: tick == 0,
            ..Default::default()
        };
        advance(&mut ship, &mut body, &input, &cmds, &Space::default());
    }
    eprintln!(
        "after decoupling without thrust: {:?} -> {:?}",
        velocity, body.lin_vel
    );
    assert!(
        (body.lin_vel - velocity).length() < 1e-8,
        "decoupling applied unwanted braking"
    );
}

#[test]
fn air_physics_is_identical_in_coupled_and_decoupled() {
    use flight_core::sc::{Modes, StepState, air};
    let t = tuning().air;
    let body = BodyState {
        lin_vel: DVec3::new(40.0, 10.0, -90.0),
        ..body_at(DVec3::Y * 10000.0)
    };
    let f = StepState::new(&body, &Air::default(), DT);
    let coupled = Modes::default();
    let decoupled = Modes {
        coupled: false,
        coupling: 0.0,
        ..coupled
    };
    let a = air::step(&mut air::AirState::default(), &f, &coupled, &t);
    let b = air::step(&mut air::AirState::default(), &f, &decoupled, &t);
    assert_eq!(
        a, b,
        "the flight-control switch must not change the air's physical forces"
    );
}

fn decoupled_ship(limiter: f64) -> ScShip {
    let mut ship = ScShip::new(instant());
    ship.modes.coupled = false;
    ship.modes.coupling = 0.0;
    ship.modes.limiter = limiter;
    ship
}

#[test]
fn releasing_each_translation_axis_preserves_the_velocity() {
    for axis in [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z] {
        let mut ship = ScShip::new(tuning());
        ship.modes.coupled = false;
        ship.modes.coupling = 0.0;
        let mut body = body_at(DVec3::Y * 10000.0);
        fly(&mut ship, &mut body, &thrust(axis), &ModeCmds::default(), &Space::default(), 1.0);
        let velocity = body.lin_vel;
        assert!(velocity.dot(axis) > 0.0);
        fly(&mut ship, &mut body, &thrust(DVec3::ZERO), &ModeCmds::default(), &Space::default(), 2.0);
        assert!((body.lin_vel - velocity).length() < 1e-8, "axis {axis:?}: release changed {velocity:?} to {:?}", body.lin_vel);
    }
}

#[test]
fn sideways_thrust_at_the_forward_cap_does_not_brake_forward_drift() {
    let mut ship = decoupled_ship(1.0);
    let cap = ship.tuning.linear.scm.cruise;
    let mut body = body_at(DVec3::Y * 10000.0);
    body.lin_vel = DVec3::NEG_Z * cap;
    fly(&mut ship, &mut body, &thrust(DVec3::X), &ModeCmds::default(), &Space::default(), 20.0);
    assert!((body.lin_vel.z + cap).abs() < 1e-8, "sideways thrust braked forward drift: {:?}", body.lin_vel);
    assert!((body.lin_vel.x - cap).abs() < 1e-8, "sideways component must reach its own cap: {:?}", body.lin_vel);
}

#[test]
fn local_axis_caps_allow_counterthrust_but_never_correct_existing_overspeed() {
    for axis in [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z] {
        let mut ship = decoupled_ship(0.1);
        let cap = ship.tuning.linear.scm.cruise * ship.modes.limiter;
        let mut body = body_at(DVec3::Y * 10000.0);
        // A turn and a lower limiter can leave a component above its new local cap.
        body.rot = DQuat::from_rotation_y(0.7);
        body.lin_vel = body.rot * axis * (cap * 2.0);
        let velocity = body.lin_vel;
        advance(&mut ship, &mut body, &thrust(axis), &ModeCmds::default(), &Space::default());
        assert!((body.lin_vel - velocity).length() < 1e-8, "outward thrust at overspeed on {axis:?}");
        advance(&mut ship, &mut body, &thrust(-axis), &ModeCmds::default(), &Space::default());
        assert!(body.lin_vel.length() < velocity.length(), "counterthrust refused on {axis:?}");
    }
}

#[test]
fn every_local_axis_stops_adding_thrust_exactly_at_its_cap() {
    for limiter in [0.1, 1.0] {
        for axis in [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z] {
            let mut ship = decoupled_ship(limiter);
            let cap = ship.tuning.linear.scm.cruise * limiter;
            let mut body = body_at(DVec3::Y * 10000.0);
            body.rot = DQuat::from_rotation_y(0.7);
            let direction = body.rot * axis;
            body.lin_vel = direction * (cap - 0.001);
            fly(&mut ship, &mut body, &thrust(axis), &ModeCmds::default(), &Space::default(), 0.5);
            assert!((body.lin_vel - direction * cap).length() < 1e-8, "axis {axis:?}, limiter {limiter}: {:?}", body.lin_vel);
        }
    }
}

#[test]
fn turning_and_lowering_the_limiter_do_not_correct_existing_drift() {
    let mut ship = decoupled_ship(1.0);
    let mut body = body_at(DVec3::Y * 10000.0);
    let cap = ship.tuning.linear.scm.cruise;
    body.lin_vel = DVec3::new(cap, 0.0, -cap);
    let velocity = body.lin_vel;
    body.rot = DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_4);
    advance(&mut ship, &mut body, &thrust(DVec3::ZERO), &ModeCmds { limiter_steps: -9, ..Default::default() }, &Space::default());
    fly(&mut ship, &mut body, &thrust(DVec3::ZERO), &ModeCmds::default(), &Space::default(), 2.0);
    assert!((body.lin_vel - velocity).length() < 1e-8);
    assert!((body.rot.inverse() * body.lin_vel).z.abs() > cap);
    assert!((ship.modes.limiter - 0.1).abs() < 1e-8);
}

#[test]
fn limiter_scales_manual_thrust_linearly_on_every_axis_including_counterthrust() {
    for axis in [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z] {
        for countering in [false, true] {
            let acceleration = |limiter| {
                let mut ship = decoupled_ship(limiter);
                let mut body = body_at(DVec3::Y * 10000.0);
                body.lin_vel = if countering { -axis * 5.0 } else { DVec3::ZERO };
                let before = body.lin_vel;
                advance(&mut ship, &mut body, &thrust(axis), &ModeCmds::default(), &Space::default());
                (body.lin_vel - before) / DT
            };
            let full = acceleration(1.0);
            for limiter in [0.1, 0.5] {
                let actual = acceleration(limiter);
                assert!((actual - full * limiter).length() < 1e-8, "axis {axis:?}, countering {countering}, limiter {limiter}: {actual:?} vs full {full:?}");
            }
        }
    }
}

#[test]
fn brake_is_independent_of_limiter_and_stops_without_reversing() {
    for g_safe in [false, true] {
        let run = |limiter| {
            let mut ship = decoupled_ship(limiter);
            ship.modes.g_safe = g_safe;
            let mut body = body_at(DVec3::Y * 10000.0);
            body.lin_vel = DVec3::new(80.0, 60.0, -200.0);
            let input = FlightInput { brake: true, piloted: true, ..Default::default() };
            let direction = -body.lin_vel.normalize();
            let full_box = ship.thrust_box(1.0, 1.0);
            let available = full_box.along(direction.signum()).abs() / direction.abs();
            let full_rate = available.min_element();
            let mut velocities = Vec::new();
            for tick in 0..1200 {
                let before = body.lin_vel;
                advance(&mut ship, &mut body, &input, &ModeCmds::default(), &Space::default());
                assert!(body.lin_vel.dot(before) >= -1e-10, "brake reversed velocity");
                if g_safe {
                    assert!(ship.status.felt_g <= flight_core::limits::G_SAFE_MAX_G + 1e-8);
                } else if tick == 0 {
                    assert!(((body.lin_vel - before) / DT - direction * full_rate).length() < 1e-8, "brake did not use full available thrust");
                }
                velocities.push(body.lin_vel);
            }
            assert_eq!(body.lin_vel, DVec3::ZERO);
            velocities
        };
        for (tick, (low, full)) in run(0.1).into_iter().zip(run(1.0)).enumerate() {
            assert_eq!(low, full, "limiter changed the brake at tick {tick}, G-safe {g_safe}");
        }
    }
}
