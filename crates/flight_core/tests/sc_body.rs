//! Numbers are printed (`cargo test -p flight_core --test sc_body -- --nocapture`).
mod sc_common;

use flight_core::limits::Dirs;
use flight_core::sc::drive::{self, Asked, DriveState, DriveTuning};
use flight_core::sc::{ModeCmds, ScShip};
use flight_core::{BodyState, FlightInput};
use glam::DVec3;
use flight_core::PlanetEnv;
use sc_common::{body_at, Space, DT};

/// The shipped tuning with caps far above what these runs reach: the checks measure the
/// thrusters (spool, jerk, boost, mass), so the coupled law must keep asking for full thrust
/// (with SC's 81 m/s² the ship reaches the 150 m/s cap in 2 s), and G-safe must not cut it.
fn tuning() -> flight_core::sc::ScTuning {
    let mut t = sc_common::tuning();
    let far = flight_core::sc::linear::Caps { cruise: 5000.0, boost_forward: 10000.0, boost_backward: 10000.0 };
    t.linear.scm = far;
    t.linear.nav = far;
    // SC's forward acceleration (81 m/s²) is above our G-safe 8 g; G-safe is the linear law's.
    t.linear.g_limit = Dirs { forward: 100.0, backward: 100.0, left: 100.0, right: 100.0, up: 100.0, down: 100.0 };
    t
}

const FWD: DVec3 = DVec3::new(0.0, 0.0, -1.0);

fn input(thrust: DVec3, boost: bool) -> FlightInput {
    FlightInput { thrust, boost, piloted: true, ..FlightInput::default() }
}

/// Flies `secs` in deep space and returns the acceleration of each step (m/s², world = ship
/// space here: the ship is level). The body is integrated as the app does.
fn run(ship: &mut ScShip, body: &mut BodyState, input: &FlightInput, secs: f64) -> Vec<DVec3> {
    run_in(ship, body, input, &Space::default(), secs)
}

fn run_in(ship: &mut ScShip, body: &mut BodyState, input: &FlightInput, env: &impl PlanetEnv, secs: f64) -> Vec<DVec3> {
    // These tests measure unconstrained hardware, not the pilot's 4 g safety ceiling.
    ship.modes.g_safe = false;
    let n = (secs / DT).round() as usize;
    let mut acc = Vec::with_capacity(n);
    for _ in 0..n {
        let before = body.lin_vel;
        let out = ship.step(body, input, &ModeCmds::default(), env, DT);
        acc.push((out.lin_vel - before) / DT);
        body.lin_vel = out.lin_vel;
        body.ang_vel = out.ang_vel;
        body.integrate(DT);
    }
    acc
}

fn mean(v: &[DVec3]) -> DVec3 {
    v.iter().copied().sum::<DVec3>() / v.len() as f64
}

fn ship_with(f: impl FnOnce(&mut flight_core::sc::ScTuning)) -> ScShip {
    let mut t = tuning();
    f(&mut t);
    ScShip::new(t)
}

#[test]
fn double_mass_halves_forward_acceleration() {
    let window = |mass: f64| {
        let mut ship = ship_with(|t| t.ship.mass = mass);
        let acc = run(&mut ship, &mut body_at(DVec3::ZERO), &input(FWD, false), 2.0);
        // Seconds 1 to 2: past the spool and the jerk ramp, still under the thrust box.
        mean(&acc[60..120]).length()
    };
    let (a, b) = (window(2000.0), window(4000.0));
    println!("forward accel 2000 kg {a:.3} m/s2, 4000 kg {b:.3} m/s2, ratio {:.4}", b / a);
    assert!((b / a - 0.5).abs() < 0.005, "ratio {}", b / a);
}

#[test]
fn cargo_adds_inertia_at_the_cabin_floor() {
    let t = tuning();
    let (i, [x, y, z]) = (t.ship.inertia, t.ship.cargo_point);
    let mut ship = ScShip::new(t);
    let before = ship.torque_box();
    ship.set_cargo_mass(1000.0);
    let after = ship.torque_box();
    // A point mass at p adds m (y² + z²) to pitch, m (x² + z²) to yaw and m (x² + y²) to roll.
    let pitch = i.pitch + 1000.0 * (y * y + z * z);
    let yaw = i.yaw + 1000.0 * (x * x + z * z);
    let roll = i.roll + 1000.0 * (x * x + y * y);
    println!("pitch box {:.2} -> {:.2} (inertia {:.0} -> {:.0})", before.pitch, after.pitch, i.pitch, pitch);
    assert!((after.pitch - before.pitch * i.pitch / pitch).abs() < 1e-9 * before.pitch.abs());
    assert!((after.yaw - before.yaw * i.yaw / yaw).abs() < 1e-9 * before.yaw.abs());
    assert!((after.roll - before.roll * i.roll / roll).abs() < 1e-9 * before.roll.abs());
}

#[test]
fn given_acceleration_changes_at_most_by_the_group_jerk_per_step() {
    let mut ship = ScShip::new(tuning());
    let jerk = ship.tuning.drive.jerk.main;
    let acc = run(&mut ship, &mut body_at(DVec3::ZERO), &input(FWD, false), 2.0);
    let worst = acc.windows(2).map(|w| (w[1].z - w[0].z).abs()).fold(0.0, f64::max);
    println!("main jerk {jerk} m/s3: largest step change {:.4} m/s2 (limit {:.4})", worst, jerk * DT);
    assert!(worst <= jerk * DT * (1.0 + 1e-9), "step change {worst} above {}", jerk * DT);
}

#[test]
fn holding_boost_at_rest_drains_by_the_idle_cost() {
    // Counted from the start of the running boost (no idle cost in the pre-delay).
    let drop = |idle: f64| {
        let mut t = DriveTuning::default();
        t.idle_cost = idle;
        let mut s = DriveState::default();
        for _ in 0..(t.boost_pre_delay / DT).ceil() as usize + 2 {
            drive::begin(&mut s, true, false, &t, DT);
        }
        let start = s.boost.charge;
        for _ in 0..(1.0 / DT) as usize {
            drive::begin(&mut s, true, false, &t, DT);
        }
        start - s.boost.charge
    };
    let (none, with) = (drop(0.0), drop(0.1));
    println!("charge drop in 1 s held at rest, running: no idle cost {none:.4}, idle 0.1/s {with:.4}, difference {:.4}", with - none);
    assert!((with - none - 0.1).abs() < 0.001, "idle cost difference {}", with - none);
}

#[test]
fn asked_thrust_is_shaped_inside_the_box_per_group() {
    // The shape stage alone: a request beyond the box is limited to the box at once only through
    // jerk, never past it, and the angular part is limited by the angular jerk per axis.
    let t = DriveTuning::default();
    let mut s = DriveState::default();
    let box_ = Dirs::splat(100.0);
    let a = Asked { linear: DVec3::new(0.0, 0.0, -100.0), angular: DVec3::new(50.0, 0.0, 0.0), boost: 0.0, thrust_box: box_, velocity: DVec3::ZERO, stick: DVec3::new(0.0, 0.0, -1.0) };
    let mut prev = drive::Given::default();
    let mut worst_ang = 0.0f64;
    for _ in 0..(3.0 / DT) as usize {
        let g = drive::shape(&mut s, &a, &t, DT);
        assert!(g.linear.z.abs() <= 100.0 + 1e-9);
        worst_ang = worst_ang.max((g.angular.x - prev.angular.x).abs());
        prev = g;
    }
    println!("angular jerk {} rad/s3: largest step change {:.4}", t.angular_jerk.pitch, worst_ang);
    assert!(worst_ang <= t.angular_jerk.pitch * DT * (1.0 + 1e-9));
    assert!((prev.angular.x - 50.0).abs() < 1e-9, "the angular request reached: {}", prev.angular.x);
}

#[test]
fn countering_boost_lands_on_zero_not_past_it() {
    let t = sc_common::instant();
    let mut ship = flight_core::sc::ScShip::new(t);
    let mut body = sc_common::body_at(DVec3::ZERO);
    // Decoupled, so only the push acts; a backward drift smaller than one step's push.
    sc_common::fly(&mut ship, &mut body, &sc_common::thrust(DVec3::ZERO), &flight_core::sc::ModeCmds { decoupled: true, ..Default::default() }, &sc_common::Space::default(), 5.0);
    body.lin_vel = DVec3::new(0.0, 0.0, 0.3);
    let i = flight_core::FlightInput { thrust: DVec3::NEG_Z, boost: true, piloted: true, ..Default::default() };
    let out = ship.step(&body, &i, &flight_core::sc::ModeCmds::default(), &sc_common::Space::default(), sc_common::DT);
    println!("countering boost from 0.3 m/s back: {:.3} m/s after one step", out.lin_vel.z);
    assert!(out.lin_vel.z.abs() < 1e-9, "z {:.3} m/s, not zero", out.lin_vel.z);
}
