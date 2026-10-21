mod sc_common;
use flight_core::sc::{ModeCmds, ScShip};
use flight_core::{BodyState, FlightInput, PlanetEnv};
use glam::{DQuat, DVec3};
use sc_common::*;

const G0: f64 = 9.81;

fn input(t: DVec3, boost: bool, brake: bool) -> FlightInput {
    FlightInput { thrust: t, boost, brake, piloted: true, ..FlightInput::default() }
}

/// One step as the app does it; returns the world acceleration the step gave the ship.
fn step(ship: &mut ScShip, body: &mut BodyState, i: &FlightInput, c: &ModeCmds, env: &impl PlanetEnv) -> DVec3 {
    let out = ship.step(body, i, c, env, DT);
    let a = (out.lin_vel - body.lin_vel) / DT;
    body.lin_vel = out.lin_vel;
    body.ang_vel = out.ang_vel;
    body.integrate(DT);
    a
}

#[test]
fn a3_the_cap_refuses_thrust_above_it() {
    let space = Space::default();
    // Each decoupled axis has its own cap: existing sideways drift must not consume
    // the forward allowance. Hold long enough to actually reach the forward cap.
    let peak = |refuse: bool| {
        let mut t = tuning();
        t.linear.refuse_thrust = refuse;
        t.modes.decouple_time = 0.0;
        let mut ship = ScShip::new(t);
        let mut body = body_at(DVec3::ZERO);
        fly(&mut ship, &mut body, &thrust(DVec3::X), &ModeCmds::default(), &space, 8.0);
        let mut peak: f64 = 0.0;
        for i in 0..(12.0 / DT) as usize {
            let c = if i == 0 { ModeCmds { decoupled: true, ..Default::default() } } else { ModeCmds::default() };
            step(&mut ship, &mut body, &input(DVec3::NEG_Z, true, false), &c, &space);
            peak = peak.max(-body.lin_vel.z);
        }
        peak
    };
    let (on, off) = (peak(true), peak(false));
    let cap = tuning().linear.scm.boost_forward;
    println!("A3 decoupled: peak with refusal {on:.2} m/s, without {off:.2} m/s, boost cap {cap}");
    assert!(on <= cap + 0.5, "peak {on:.2} above the cap {cap} + 0.5");
    assert!(off > on + 5.0, "without refusal the peak is higher: {off:.2} vs {on:.2}");
}

#[test]
fn gravity_compensation_off_falls_with_g_and_on_again_holds() {
    let env = Air::default();
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::new(0.0, 500.0, 0.0));
    fly(&mut ship, &mut body, &FlightInput::default(), &ModeCmds { grav_comp: true, ..Default::default() }, &env, 1.0);
    assert!(!ship.status.grav_comp);
    let (fallen, speed) = (500.0 - body.pos.y, -body.lin_vel.y);
    let g = G0;
    println!("H off, 1 s: fell {fallen:.3} m (g/2 = {:.3}), speed {speed:.3} (g = {g})", g / 2.0);
    assert!((fallen - g / 2.0).abs() < 0.02 * g / 2.0, "fell {fallen:.3} m");
    assert!((speed - g).abs() < 0.02 * g, "speed {speed:.3}");

    fly(&mut ship, &mut body, &FlightInput::default(), &ModeCmds { grav_comp: true, ..Default::default() }, &env, 3.0);
    assert!(ship.status.grav_comp);
    println!("H on, 3 s later: vertical speed {:.3} m/s", body.lin_vel.y);
    assert!(body.lin_vel.y.abs() < 1.0, "vertical speed {:.3} after compensation on", body.lin_vel.y);
}

#[test]
fn g_safe_keeps_the_forward_felt_g_under_the_limit_in_a_boost() {
    let space = Space::default();
    let peak = |g_safe: bool| {
        let mut ship = ScShip::new(tuning());
        let mut body = body_at(DVec3::ZERO);
        let first = if g_safe { ModeCmds::default() } else { ModeCmds { g_safe: true, ..Default::default() } };
        let mut peak: f64 = 0.0;
        for i in 0..(3.0 / DT) as usize {
            let c = if i == 0 { first } else { ModeCmds::default() };
            let a = step(&mut ship, &mut body, &input(DVec3::NEG_Z, true, false), &c, &space);
            peak = peak.max(-a.z);
        }
        (peak, ship.tuning.linear.g_limit.forward * G0)
    };
    let (on, limit) = peak(true);
    let (off, _) = peak(false);
    println!("G-safe: forward felt {on:.2} m/s2 on, {off:.2} off, limit {limit:.2}");
    assert!(on <= limit + 1e-6, "felt {on:.2} above {limit:.2}");
    assert!(off > on + 1.0, "off {off:.2} not above on {on:.2}");
}

// lag is TODO(initiator).

#[test]
fn turning_at_rest_launches_along_the_nose_with_comstab_on_or_off() {
    let env = Space::default();
    for comstab in [true, false] {
        for speed in [0.0, 0.004] {
            let mut ship = ScShip::new(tuning());
            ship.modes.comstab = comstab;
            let mut body = body_at(DVec3::ZERO);
            body.lin_vel = DVec3::NEG_Z * speed;
            // Turn on the spot without thrust, including a nonzero speed rounded to HUD 0.00.
            for i in 0..120 {
                body.rot = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2 * (i + 1) as f64 / 120.0);
                fly(&mut ship, &mut body, &input(DVec3::ZERO, false, false), &ModeCmds::default(), &env, DT);
            }
            let nose = body.rot * DVec3::NEG_Z;
            let start = body.pos;
            fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &env, 1.0);
            let across = body.lin_vel - nose * body.lin_vel.dot(nose);
            let travel = body.pos - start;
            let across_travel = travel - nose * travel.dot(nose);
            assert!(body.lin_vel.dot(nose) > 20.0, "launch accelerates along the nose");
            assert!(across.length() < 0.001 && across_travel.length() < 0.001,
                "comstab {comstab}, initial speed {speed}: velocity {across:?}, travel {across_travel:?} across the nose");
        }
    }
}

#[test]
fn comstab_off_after_braking_and_turning_does_not_remember_the_old_heading() {
    let env = Space::default();
    let mut ship = ScShip::new(tuning());
    ship.modes.comstab = false;
    let mut body = body_at(DVec3::ZERO);
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &env, 5.0);
    for _ in 0..600 {
        step(&mut ship, &mut body, &input(DVec3::ZERO, false, true), &ModeCmds::default(), &env);
        if body.lin_vel == DVec3::ZERO {
            break;
        }
    }
    assert_eq!(body.lin_vel, DVec3::ZERO, "braking reaches exact rest within 10 s");
    for i in 0..120 {
        body.rot = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2 * (i + 1) as f64 / 120.0);
        fly(&mut ship, &mut body, &input(DVec3::ZERO, false, false), &ModeCmds::default(), &env, DT);
    }
    let nose = body.rot * DVec3::NEG_Z;
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &env, 1.0);
    assert!(body.lin_vel.dot(nose) > 20.0);
    assert!(body.lin_vel.angle_between(nose).to_degrees() < 0.01, "velocity {:?} follows the nose {nose:?}", body.lin_vel);
}

#[test]
fn slow_decoupled_motion_is_not_snapped_to_rest_without_braking() {
    let env = Space::default();
    let mut ship = ScShip::new(tuning());
    ship.modes.coupled = false;
    ship.modes.coupling = 0.0;
    let mut body = body_at(DVec3::ZERO);
    body.lin_vel = DVec3::NEG_Z * 0.004;
    let velocity = body.lin_vel;
    fly(&mut ship, &mut body, &input(DVec3::ZERO, false, false), &ModeCmds::default(), &env, 2.0);
    assert_eq!(body.lin_vel, velocity, "slow unbraked motion stays physical");
}

/// Holds Ctrl (down) from 200 m with the given proximity switch. Returns (lowest clearance seen
/// before the ground, speed when the height first drops under 1 m, the step it reached the ground).
fn dive(proximity: bool) -> (f64, f64, Option<f64>) {
    let env = Air::default();
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::new(0.0, 200.0, 0.0));
    let first = if proximity { ModeCmds::default() } else { ModeCmds { proximity: true, ..Default::default() } };
    let (mut low, mut speed_near_ground, mut ground_at) = (f64::MAX, None::<f64>, None);
    for i in 0..(60.0 / DT) as usize {
        let c = if i == 0 { first } else { ModeCmds::default() };
        step(&mut ship, &mut body, &input(DVec3::NEG_Y, false, false), &c, &env);
        if body.pos.y > 0.0 {
            low = low.min(body.pos.y);
        }
        if body.pos.y < 1.0 && speed_near_ground.is_none() {
            speed_near_ground = Some(body.lin_vel.length());
        }
        if body.pos.y <= 0.0 {
            ground_at = Some(i as f64 * DT);
            break;
        }
    }
    (low, speed_near_ground.unwrap_or(f64::NAN), ground_at)
}

#[test]
fn proximity_keeps_the_ship_off_the_ground_in_a_dive() {
    let (low, _, _) = dive(true);
    assert!(low > 0.0, "reached the ground: clearance {low:.3}");
}
