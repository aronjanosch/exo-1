//! The SC model's angular stage (round 5, lane `sc-angular`, issue #196): one budget, a second-order
//! pitch and yaw, reversal and roll release as constant decelerations, rate over speed, the G-safe
//! turn cap, landing mode and the strong and weak axes. Numbers are printed (`--nocapture`).
mod sc_common;
use flight_core::sc::angular::{self, AngularState, AngularTuning, Env};
use flight_core::sc::modes::Modes;
use flight_core::sc::{ModeCmds, ScShip};
use flight_core::sc::Frame;
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
    (ScShip::new(tuning()), body_at(DVec3::new(0.0, 500.0, 0.0)), Space::default())
}

#[test]
fn pitch_overshoots_five_to_twenty_percent_and_settles() {
    let (mut ship, mut body, env) = at_rest();
    let target = rest_rates(&ship.tuning.angular).x;
    let spins = run(&mut ship, &mut body, &stick(1.0, 0.0, 0.0), &env, 2.5);
    let peak = spins.iter().map(|s| s.x).fold(f64::MIN, f64::max);
    let over = peak / target - 1.0;
    let at2 = spins[(2.0 / DT) as usize].x;
    let settle = (at2 / target - 1.0).abs();
    eprintln!("pitch: target {target:.4} rad/s, peak {peak:.4} ({:+.1} %), at 2 s {at2:.4} ({:+.2} %)", over * 100.0, (at2 / target - 1.0) * 100.0);
    assert!((0.05..=0.20).contains(&over), "overshoot {:.1} % outside 5 to 20 %", over * 100.0);
    assert!(settle < 0.02, "2 s after the stick: {:.2} % off the target", settle * 100.0);
}

#[test]
fn reversal_is_a_constant_deceleration_at_the_reversal_share() {
    let (mut ship, mut body, env) = at_rest();
    run(&mut ship, &mut body, &stick(1.0, 0.0, 0.0), &env, 3.0);
    let box_ = ship.torque_box().pitch;
    let share = ship.tuning.angular.reversal_share;
    let before = body.rot.inverse() * body.ang_vel;
    let spins = run(&mut ship, &mut body, &stick(-1.0, 0.0, 0.0), &env, 2.0);
    // Per tick decelerations while the spin is still on its side (before it crosses zero).
    let mut prev = before.x;
    let mut decel = Vec::new();
    for s in &spins {
        if s.x <= 0.0 {
            break;
        }
        decel.push((s.x - prev) / DT);
        prev = s.x;
    }
    assert!(decel.len() > 10, "reversal ran {} ticks", decel.len());
    let mean = decel.iter().sum::<f64>() / decel.len() as f64;
    let spread = decel.iter().map(|a| (a - mean).abs() / mean.abs()).fold(0.0, f64::max);
    eprintln!("reversal: {} ticks, mean decel {mean:.3} rad/s^2 (share {share} of box {box_:.3} = {:.3}), spread {:.2} %", decel.len(), share * box_, spread * 100.0);
    assert!((mean + share * box_).abs() < 0.05 * share * box_, "mean decel {mean:.3}, want {:.3}", -share * box_);
    assert!(spread < 0.05, "decel spread {:.2} % over 5 %", spread * 100.0);
}

#[test]
fn roll_release_is_a_constant_deceleration_that_stops_without_overshoot() {
    let (mut ship, mut body, env) = at_rest();
    run(&mut ship, &mut body, &stick(0.0, 0.0, 1.0), &env, 2.0);
    let rolled = (body.rot.inverse() * body.ang_vel).z;
    let box_ = ship.torque_box().roll;
    let share = ship.tuning.angular.roll_release_share;
    let spins = run(&mut ship, &mut body, &stick(0.0, 0.0, 0.0), &env, 1.5);
    let mut prev = rolled;
    let mut decel = Vec::new();
    for s in &spins {
        if s.z <= 0.0 {
            break;
        }
        decel.push((s.z - prev) / DT);
        prev = s.z;
    }
    let min = spins.iter().map(|s| s.z).fold(f64::MAX, f64::min);
    let stopped = spins.iter().position(|s| s.z.abs() < 1e-9);
    let mean = decel.iter().sum::<f64>() / decel.len().max(1) as f64;
    let spread = decel.iter().map(|a| (a - mean).abs() / mean.abs()).fold(0.0, f64::max);
    eprintln!("roll release from {rolled:.3} rad/s: {} ticks of decel, mean {mean:.3} rad/s^2 (share {share} of box {box_:.3}), spread {:.2} %, stops at tick {stopped:?}, lowest spin {min:.4}", decel.len(), spread * 100.0);
    assert!(decel.len() > 5, "release ran {} ticks", decel.len());
    assert!((mean + share * box_).abs() < 0.05 * share * box_, "mean decel {mean:.3}");
    assert!(spread < 0.05, "decel spread {:.2} %", spread * 100.0);
    assert!(stopped.is_some(), "the roll stops");
    assert!(min > -0.01, "overshoot past zero: {min:.4} rad/s");
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
fn g_safe_caps_a_full_pitch_at_cruise_and_off_it_does_not() {
    let env = Space::default();
    let cruise_thrust = FlightInput { thrust: DVec3::new(0.0, 0.0, -1.0), piloted: true, ..Default::default() };
    let mut on = ScShip::new(tuning());
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

    let mut off = ScShip::new(tuning());
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
    assert!(!off.status.rate_capped, "G-safe off does not cap");
    assert!(spin_off.x > 1.5 * spin_on.x, "off turns faster: {:.4} vs {:.4}", spin_off.x, spin_on.x);
}

/// The pitch target a frame asks for, read from the first step of the spring at rest (the box made
/// huge): the spring's acceleration is `dt * natural_frequency² * target` then.
fn pitch_target(t: &AngularTuning, speed: f64, cap: f64, landing: bool, clearance: f64, g_safe: bool) -> f64 {
    let lv = DVec3::new(0.0, 0.0, -speed);
    let f = Frame {
        rot: DQuat::IDENTITY,
        inv: DQuat::IDENTITY,
        pos: DVec3::ZERO,
        v: lv,
        lv,
        w_local: DVec3::ZERO,
        gravity: DVec3::ZERO,
        up: DVec3::Y,
        density: 0.0,
        altitude: 0.0,
        clearance,
        dt: DT,
    };
    let input = FlightInput { turn: DVec2::new(1.0, 0.0), piloted: true, ..Default::default() };
    let modes = Modes { g_safe, landing, ..Modes::default() };
    let env = Env { accel_box: flight_core::axis::Rot { pitch: 1e6, yaw: 1e6, roll: 1e6 }, boost: 0.0, cap, thrust_box: flight_core::axis::Dirs::splat(1e6) };
    let out = angular::step(&mut AngularState::default(), &f, &input, &modes, &env, t);
    out.accel.x / (DT * t.natural_frequency.pitch * t.natural_frequency.pitch)
}

#[test]
fn rate_at_half_the_cap_beats_rest_and_the_cap() {
    let t = tuning().angular;
    let cap = 150.0;
    let rest = pitch_target(&t, 0.0, cap, false, 1e3, false);
    let half = pitch_target(&t, 0.5 * cap, cap, false, 1e3, false);
    let top = pitch_target(&t, cap, cap, false, 1e3, false);
    eprintln!("corner: rate at rest {rest:.4}, at 0.5 x cap {half:.4}, at the cap {top:.4} rad/s");
    assert!(half > rest, "0.5 x cap {half:.4} vs rest {rest:.4}");
    assert!(half > top, "0.5 x cap {half:.4} vs cap {top:.4}");
}

#[test]
fn landing_mode_scales_the_rates_near_the_ground() {
    let t = tuning().angular;
    let free = pitch_target(&t, 0.0, 150.0, false, 2.0, false);
    let low = pitch_target(&t, 0.0, 150.0, true, 2.0, false);
    let mid = pitch_target(&t, 0.0, 150.0, true, 20.0, false);
    let high = pitch_target(&t, 0.0, 150.0, true, 60.0, false);
    eprintln!("landing: rate ratio at 2 m {:.3}, at 20 m {:.3}, at 60 m {:.3}", low / free, mid / free, high / free);
    assert!((low / free - t.landing_rate_share).abs() < 1e-9, "2 m: {:.4}", low / free);
    assert!(mid / free > t.landing_rate_share + 1e-3 && mid / free < 1.0 - 1e-3, "20 m between the shares: {:.4}", mid / free);
    assert!((high / free - 1.0).abs() < 1e-9, "60 m: {:.4}", high / free);
}

#[test]
fn yaw_steady_rate_is_below_pitch_with_the_shipped_tuning() {
    let (mut ship, mut body, env) = at_rest();
    let pitch = run(&mut ship, &mut body, &stick(1.0, 0.0, 0.0), &env, 3.0).last().unwrap().x;
    let (mut ship, mut body, env) = at_rest();
    let yaw = run(&mut ship, &mut body, &stick(0.0, 1.0, 0.0), &env, 3.0).last().unwrap().y;
    eprintln!("steady rates: pitch {pitch:.4}, yaw {yaw:.4} rad/s (torque boxes {:.3} and {:.3} rad/s^2)", ship.torque_box().pitch, ship.torque_box().yaw);
    assert!(yaw.abs() < pitch.abs(), "yaw {yaw:.4} not below pitch {pitch:.4}");
}

#[test]
fn acceleration_stays_inside_the_box() {
    let t = tuning().angular;
    let f = Frame {
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
    let env = Env { accel_box: flight_core::axis::Rot { pitch: 0.5, yaw: 0.25, roll: 0.75 }, boost: 0.0, cap: 150.0, thrust_box: flight_core::axis::Dirs::splat(1e6) };
    let out = angular::step(&mut AngularState::default(), &f, &input, &Modes::default(), &env, &t);
    eprintln!("box: accel {:?} inside (0.5, 0.25, 0.75)", out.accel);
    assert!(out.accel.x.abs() <= 0.5 && out.accel.y.abs() <= 0.25 && out.accel.z.abs() <= 0.75);
}
