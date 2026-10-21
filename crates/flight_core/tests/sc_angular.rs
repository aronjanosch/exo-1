mod sc_common;
use flight_core::sc::angular::{self, AngularState, AngularTuning, Env};
use flight_core::sc::modes::Modes;
use flight_core::sc::{ModeCmds, ScShip};
use flight_core::sc::StepState;
use flight_core::{BodyState, FlightInput, PlanetEnv};
use glam::{DQuat, DVec2, DVec3};
use sc_common::*;

/// Ship-space spin of the body after each tick of `secs` with `input`.
fn run(ship: &mut ScShip, body: &mut BodyState, input: &FlightInput, env: &impl PlanetEnv, secs: f64) -> Vec<DVec3> {
    let n = (secs / DT).round() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let o = ship.step(body, input, &ModeCmds::default(), env, DT);
        body.lin_vel = o.lin_vel;
        body.ang_vel = o.ang_vel;
        body.integrate(DT);
        out.push(body.rot.inverse() * body.ang_vel);
    }
    out
}

fn stick(pitch: f64, yaw: f64, roll: f64) -> FlightInput {
    FlightInput { turn: DVec2::new(pitch, yaw), roll, piloted: true, ..Default::default() }
}

/// The rates at rest (over-speed share at speed 0, no boost): what full stick asks for.
fn rest_rates(t: &AngularTuning) -> DVec3 {
    let over = t.rate_over_speed.eval(0.0);
    DVec3::new(t.rate.pitch * over, t.rate.yaw * over, t.rate.roll * over)
}

fn at_rest() -> (ScShip, BodyState, Space) {
    (ScShip::new(instant()), body_at(DVec3::new(0.0, 500.0, 0.0)), Space::default())
}

#[test]
fn diagonal_pitch_yaw_roll_stays_on_one_budget() {
    let (mut ship, mut body, env) = at_rest();
    let rates = rest_rates(&ship.tuning.angular);
    let boxes = ship.torque_box();
    let limit = DVec3::new(boxes.pitch, boxes.yaw, boxes.roll);
    let mut prev = DVec3::ZERO;
    let mut worst_accel: f64 = 0.0;
    let spins = run(&mut ship, &mut body, &stick(1.0, 1.0, 1.0), &env, 3.0);
    for s in &spins {
        worst_accel = worst_accel.max(((*s - prev) / DT / limit).abs().max_element());
        prev = *s;
    }
    let steady = *spins.last().unwrap();
    let norm = (steady / rates).length();
    eprintln!("diagonal: steady spin {steady:.4?} rad/s, ellipsoid norm {norm:.4}, largest accel / box {worst_accel:.4}");
    assert!(norm <= 1.05, "ellipsoid norm {norm:.4} over 1.05");
    assert!(worst_accel <= 1.0 + 1e-9, "accel leaves the torque box: {worst_accel:.4} of it");
}

#[test]
fn g_safe_lowers_the_coupled_turn_budget_when_thrusters_exceed_the_g_limit() {
    let env = Space::default();
    let cruise_thrust = FlightInput { thrust: DVec3::new(0.0, 0.0, -1.0), piloted: true, ..Default::default() };
    // Isolate the G limit: the shipped vertical thrusters are already below it.
    let mut strong = tuning();
    strong.ship.thrust.up *= 4.0;
    strong.ship.thrust.down *= 4.0;
    let mut on = ScShip::new(strong.clone());
    let mut body = body_at(DVec3::ZERO);
    run(&mut on, &mut body, &cruise_thrust, &env, 8.0);
    let speed = body.lin_vel.length();
    let turning = FlightInput { turn: DVec2::new(1.0, 0.0), ..cruise_thrust };
    let spins_on = run(&mut on, &mut body, &turning, &env, 3.0);
    let (capped, spin_on, lv) = (on.status.rate_capped, *spins_on.last().unwrap(), body.rot.inverse() * body.lin_vel);
    // Felt push of the turn: rate x velocity (ship space) against the limit per direction.
    let turn = DVec3::new(spin_on.x, spin_on.y, 0.0).cross(lv);
    let g = on.tuning.angular.g_limit.scaled(9.81);
    let worst = [(turn.x, g.right, g.left), (turn.y, g.up, g.down), (turn.z, g.backward, g.forward)]
        .iter()
        .map(|&(k, pos, neg)| if k > 0.0 { k / pos } else { -k / neg })
        .fold(0.0, f64::max);
    eprintln!("G-safe on at {speed:.1} m/s: capped {capped}, pitch {:.4} rad/s, push {:.2} of the limit", spin_on.x, worst);
    assert!(speed > 0.8 * on.tuning.linear.scm.cruise, "fixture: cruise speed {speed:.1} m/s");
    assert!(capped, "G-safe cut the rate");
    assert!(worst <= 1.02, "the turn's push {worst:.3} of the limit");

    let mut off = ScShip::new(strong);
    let mut body = body_at(DVec3::ZERO);
    run(&mut off, &mut body, &cruise_thrust, &env, 8.0);
    let mut cmd = ModeCmds { g_safe: true, ..Default::default() };
    cmd.limiter_steps = 0;
    let out = off.step(&body, &FlightInput { turn: DVec2::new(1.0, 0.0), ..cruise_thrust }, &cmd, &env, DT);
    body.lin_vel = out.lin_vel;
    body.ang_vel = out.ang_vel;
    run(&mut off, &mut body, &turning, &env, 3.0);
    let spin_off = body.rot.inverse() * body.ang_vel;
    eprintln!("G-safe off at {:.1} m/s: capped {}, pitch {:.4} rad/s", body.lin_vel.length(), off.status.rate_capped, spin_off.x);
    assert!(spin_off.x > 1.5 * spin_on.x, "off turns faster: {:.4} vs {:.4}", spin_off.x, spin_on.x);
}

#[test]
fn acceleration_stays_inside_the_box() {
    let t = tuning().angular;
    let f = StepState {
        rot: DQuat::IDENTITY,
        inv: DQuat::IDENTITY,
        pos: DVec3::ZERO,
        v: DVec3::ZERO,
        lv: DVec3::ZERO,
        w_local: DVec3::new(-0.3, 0.2, 0.0),
        gravity: DVec3::ZERO,
        up: DVec3::Y,
        density: 0.0,
        altitude: 0.0,
        clearance: 1e3,
        dt: DT,
    };
    let input = FlightInput { turn: DVec2::new(-1.0, 1.0), roll: -1.0, piloted: true, ..Default::default() };
    let env = Env { accel_box: flight_core::limits::Rot { pitch: 0.5, yaw: 0.25, roll: 0.75 }, boost: 0.0, cap: 150.0, thrust_box: flight_core::limits::Dirs::splat(1e6), linear_hold: DVec3::ZERO, priority: None, air_hold: DVec3::ZERO, level: false };
    let out = angular::step(&mut AngularState::default(), &f, &input, &Modes::default(), &env, &t);
    eprintln!("box: accel {:?} inside (0.5, 0.25, 0.75)", out.accel);
    assert!(out.accel.x.abs() <= 0.5 && out.accel.y.abs() <= 0.25 && out.accel.z.abs() <= 0.75);
}

#[test]
fn shared_turn_allocation_retains_the_absolute_axis_and_total_g_limits() {
    use flight_core::limits::{Dirs, G0, G_SAFE_MAX_G, Rot};
    let t = tuning().angular;
    let axes = Dirs { left: 25.0, right: 30.0, up: 35.0, down: 20.0, forward: 60.0, backward: 40.0 };
    for g_safe in [false, true] {
        for velocity in [DVec3::NEG_Z * 100.0, DVec3::new(30.0, -20.0, -90.0)] {
            for hold in [DVec3::ZERO, DVec3::Y * G0, DVec3::new(-5.0, 5.0, 4.0)] {
                for spin in [-0.2, 0.2] {
                    let body = BodyState { lin_vel: velocity, ang_vel: DVec3::new(spin, -spin, 0.0), ..body_at(DVec3::Y * 10000.0) };
                    let f = StepState::new(&body, &Space::default(), DT);
                    let e = Env { accel_box: Rot { pitch: 10.0, yaw: 10.0, roll: 10.0 }, boost: 0.0, cap: 150.0,
                        thrust_box: axes, linear_hold: hold, priority: Some(DVec3::new(3.0, 3.0, -10.0)), air_hold: DVec3::ZERO, level: false };
                    let out = angular::step(&mut AngularState::default(), &f, &stick(-1.0, 1.0, 0.0), &Modes { g_safe, ..Default::default() }, &e, &t);
                    let required = hold + DVec3::new(out.rate.x, out.rate.y, 0.0).cross(velocity);
                    let limit = if g_safe { axes.min(&t.g_limit.scaled(G0)) } else { axes };
                    assert!((limit.clamp(required) - required).length() < 1e-8, "turn left axis budget: {required:?}");
                    if g_safe {
                        assert!(required.length() <= G_SAFE_MAX_G * G0 + 1e-8, "turn exceeded total G-safe: {required:?}");
                    }
                }
            }
        }
    }
}
