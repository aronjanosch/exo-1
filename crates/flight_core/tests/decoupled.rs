mod sc_common;
use flight_core::sc::{ModeCmds, ScShip};
use flight_core::{BodyState, FlightInput, PlanetEnv};
use glam::{DVec2, DVec3};
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
