//! Momentum preservation through the actual flight controller, including mode switches and mouse.
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
fn atmospheric_decoupled_mouse_rotation_at_cruise_loses_only_a_little_speed() {
    let mut ship = ScShip::new(tuning());
    ship.modes.coupled = false;
    ship.modes.coupling = 0.0;
    let cruise = ship.tuning.linear.scm.cruise * ship.tuning.air.cap_share;
    let velocity = DVec3::NEG_Z * cruise;
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
    let env = Air::default();
    for _ in 0..600 {
        advance(&mut ship, &mut body, &input, &ModeCmds::default(), &env);
    }
    let speed = body.lin_vel.length();
    let angle = body.lin_vel.angle_between(velocity).to_degrees();
    eprintln!(
        "atmospheric coasting with rotation: {cruise:.2} -> {speed:.2} m/s, heading change {angle:.2} degrees"
    );
    // Provisional acceptance band for 'weak' atmospheric braking, not final balancing.
    assert!(
        speed > cruise * 0.9 && speed < cruise,
        "speed loss should be weak but nonzero: {speed:.2}"
    );
    assert!(
        angle < 5.0,
        "rotation redirected the flight path by {angle:.2} degrees"
    );
}

#[test]
fn decoupled_compensates_strong_lift_as_far_as_the_thrusters_allow() {
    use flight_core::sc::{StepState, air};
    use glam::DQuat;
    let mut ship = ScShip::new(tuning());
    ship.modes.coupled = false;
    ship.modes.coupling = 0.0;
    // Isolate aerodynamic compensation from gravity, wind and terrain turbulence.
    ship.tuning.air.wind_speed = 0.0;
    ship.tuning.air.gust_speed = 0.0;
    let env = Air {
        g: 0.0,
        ..Default::default()
    };
    let mut body = BodyState {
        rot: DQuat::from_rotation_x(15_f64.to_radians()),
        lin_vel: DVec3::NEG_Z * 150.0,
        ..body_at(DVec3::Y * 10000.0)
    };
    let input = FlightInput {
        piloted: true,
        ..Default::default()
    };
    let (mut passive, mut remaining, mut counter, mut available) = (0.0, 0.0, 0.0, 0.0);
    for _ in 0..15 {
        let f = StepState::new(&body, &env, DT);
        let air = air::step(&mut ship.air.clone(), &f, &ship.modes, &ship.tuning.air);
        let direction = body.lin_vel.normalize();
        let side = |a: DVec3| (a - direction * a.dot(direction)).length();
        passive = side(air.accel);
        let out = ship.step(&body, &input, &ModeCmds::default(), &env, DT);
        remaining = side((out.lin_vel - body.lin_vel) / DT);
        let cruise = ship.tuning.linear.scm.cruise * air.cap_scale;
        let taper = ship
            .tuning
            .linear
            .strafe_taper
            .eval((-f.lv.z).max(0.0) / cruise);
        available = ship.thrust_box(air.thrust_scale, 0.0).down * taper;
        counter = -(f.inv * out.force / ship.mass()).y;
        assert!(out.force.length() / ship.mass() <= 4.0 * 9.81 + 1e-9);
        body.lin_vel = out.lin_vel;
        body.ang_vel = out.ang_vel;
        body.integrate(DT);
    }
    eprintln!(
        "strong lift: passive {:.2} g, remaining lateral acceleration {:.2} g",
        passive / 9.81,
        remaining / 9.81
    );
    assert!(passive > 9.81, "fixture needs substantial lift");
    assert!(remaining < passive, "compensation should reduce deflection");
    assert!(
        counter >= available * 0.95,
        "use the available counter-thrust: {counter:.2} / {available:.2} m/s²"
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
