//! The SC model's air stage (round 5, #199): drag per axis, lift over the angle of attack,
//! weathervaning, wind with wind compensation, turbulence near the ground, and space.
//! Numbers are printed (`cargo test -p flight_core sc_air -- --nocapture`).
mod sc_common;

use flight_core::sc::air::{self, AirState, Axes};
use flight_core::sc::{Frame, ModeCmds, Modes, ScShip};
use flight_core::FlightInput;
use glam::DVec3;
use sc_common::*;

/// The air stage on a body at `pos` moving with `vel` (world), in an air planet with no gravity.
fn air_at(pos: DVec3, vel: DVec3, env: &Air, t: &flight_core::sc::air::AirTuning) -> air::AirOut {
    let mut b = body_at(pos);
    b.lin_vel = vel;
    let f = Frame::new(&b, env, DT);
    air::step(&mut AirState::default(), &f, &Modes::default(), t)
}

/// Air with no gravity, no wind and no drag or lift, to read one term at a time.
fn quiet() -> flight_core::sc::air::AirTuning {
    let mut t = tuning().air;
    t.wind_speed = 0.0;
    t.gust_speed = 0.0;
    t.drag = Axes { x: 0.0, y: 0.0, z: 0.0 };
    t.lift_k = 0.0;
    t.turbulence_angular = 0.0;
    t.turbulence_linear = 0.0;
    t
}

fn no_gravity() -> Air {
    Air { g: 0.0, ..Air::default() }
}

#[test]
fn drag_sideways_brakes_more_than_nose_first() {
    let env = no_gravity();
    let mut t = quiet();
    t.drag = tuning().air.drag;
    let side = air_at(DVec3::Y * 100.0, DVec3::X * 50.0, &env, &t).accel.length();
    let nose = air_at(DVec3::Y * 100.0, DVec3::NEG_Z * 50.0, &env, &t).accel.length();
    println!("drag at 50 m/s: sideways {side:.2} m/s², nose first {nose:.2} m/s²");
    assert!(side > nose, "sideways {side} should exceed nose first {nose}");
    assert!(side > 2.0 * nose, "sideways drag coefficient is 4x the nose one: {side} vs {nose}");
}

#[test]
fn drag_opposes_the_airspeed_and_grows_with_its_square() {
    let env = no_gravity();
    let mut t = quiet();
    t.drag = tuning().air.drag;
    let a25 = air_at(DVec3::Y * 100.0, DVec3::NEG_Z * 25.0, &env, &t).accel;
    let a50 = air_at(DVec3::Y * 100.0, DVec3::NEG_Z * 50.0, &env, &t).accel;
    println!("nose drag: 25 m/s {:.3}, 50 m/s {:.3}", a25.z, a50.z);
    assert!(a50.z > 0.0, "drag pushes back against a forward airspeed (+z is back): {a50}");
    assert!((a50.z / a25.z - 4.0).abs() < 1e-9, "twice the speed, four times the drag: {}", a50.z / a25.z);
}

#[test]
fn lift_rises_to_the_stall_angle_and_drops_after_it() {
    let env = no_gravity();
    let mut t = quiet();
    t.lift_k = tuning().air.lift_k;
    t.lift_curve = tuning().air.lift_curve;
    let stall = 15.0;
    let lift_at = |deg: f64| {
        let a = deg.to_radians();
        // Nose above the airflow by `deg`: the airspeed points below the nose.
        let v = DVec3::new(0.0, -50.0 * a.sin(), -50.0 * a.cos());
        air_at(DVec3::Y * 100.0, v, &env, &t).accel.y
    };
    let rows: Vec<(f64, f64)> = [0.0, 5.0, 10.0, 15.0, 25.0, 35.0].iter().map(|&d| (d, lift_at(d))).collect();
    for (d, l) in &rows {
        println!("lift at {d:>4.0} deg, 50 m/s: {l:.3} m/s²");
    }
    assert!(lift_at(0.0).abs() < 1e-9, "no lift at zero angle");
    assert!(lift_at(5.0) < lift_at(10.0) && lift_at(10.0) < lift_at(stall), "lift rises up to the stall angle");
    assert!(lift_at(stall + 10.0) < lift_at(stall) - 1.0, "10 deg past the stall the lift is lower: {} vs {}", lift_at(stall + 10.0), lift_at(stall));
    // Negative angle: lift points down.
    assert!(lift_at(-10.0) < -lift_at(10.0) + 1e-9 && lift_at(-10.0) < 0.0, "lift is odd in the angle");
}

#[test]
fn weathervaning_turns_the_nose_towards_the_airspeed() {
    let env = no_gravity();
    // No wind: the airspeed is the ship's velocity.
    let t = quiet();
    // Moving right of the nose (airspeed to +x): the nose must turn right, a negative yaw rate.
    let right = air_at(DVec3::Y * 100.0, DVec3::new(10.0, 0.0, -50.0), &env, &t).angular;
    // Moving up of the nose (airspeed to +y): the nose must come up, a positive pitch rate.
    let up = air_at(DVec3::Y * 100.0, DVec3::new(0.0, 10.0, -50.0), &env, &t).angular;
    let straight = air_at(DVec3::Y * 100.0, DVec3::new(0.0, 0.0, -50.0), &env, &t).angular;
    println!("weathervane: sideslip right -> yaw {:.4} rad/s², up -> pitch {:.4} rad/s², straight {:?}", right.y, up.x, straight);
    assert!(right.y < 0.0, "yaw towards the airspeed: {}", right.y);
    assert!(up.x > 0.0, "pitch towards the airspeed: {}", up.x);
    assert!(straight.x.abs() < 1e-12 && straight.y.abs() < 1e-12, "nose first: nothing to turn");
}

#[test]
fn wind_held_with_wind_compensation_and_pushes_without() {
    let env = Air::default();
    let t = tuning().air;
    let mut b = body_at(DVec3::Y * 50.0);
    b.lin_vel = DVec3::ZERO;
    let f = Frame::new(&b, &env, DT);
    let on = air::step(&mut AirState::default(), &f, &Modes { wind_comp: true, ..Modes::default() }, &t);
    let off = air::step(&mut AirState::default(), &f, &Modes { wind_comp: false, ..Modes::default() }, &t);
    println!("wind {:.2} m/s at rest: held {:.3} m/s², pushed {:.3} m/s²", on.wind.length(), on.accel.length(), off.push.length());
    assert!(on.wind.length() > 10.0, "a 15 m/s wind (gusts aside): {:?}", on.wind);
    assert!(on.push == DVec3::ZERO, "with wind compensation nothing is pushed");
    assert!(on.accel.length() > 0.1, "the wind force is held: {:?}", on.accel);
    assert!(off.accel == DVec3::ZERO, "without wind compensation the wind is not held");
    assert!((off.push - on.accel).length() < 1e-9, "the same force, on the other list");
}

#[test]
fn hover_with_wind_compensation_holds_and_without_it_drifts_downwind() {
    let env = Air::default();
    let start = DVec3::Y * 50.0;
    let t = tuning();
    let drift = |wind_comp_on: bool| {
        let mut ship = ScShip::new(t.clone());
        let mut b = body_at(start);
        // The first step taps the switch: the default is on, a tap turns it off.
        let cmds = ModeCmds { wind_comp: !wind_comp_on, ..ModeCmds::default() };
        fly(&mut ship, &mut b, &FlightInput::default(), &cmds, &env, 5.0);
        (b.pos - start).length()
    };
    let on = drift(true);
    let off = drift(false);
    println!("hover 5 s at 50 m: drift with wind compensation {on:.2} m, without {off:.2} m");
    assert!(on < 1.0, "hover with wind compensation drifts {on} m");
    assert!(off > 5.0, "hover without wind compensation drifts only {off} m");
}

#[test]
fn turbulence_near_the_ground_at_speed_and_none_above_twice_the_height() {
    let env = Air::default();
    let t = tuning().air;
    let h = t.turbulence_height;
    let at = |clearance: f64, speed: f64| air_at(DVec3::Y * clearance, DVec3::new(0.0, 0.0, -speed), &env, &t).turbulence;
    let low = at(20.0, 100.0);
    println!("turbulence: 20 m, 100 m/s {low:.2}; {} m, 100 m/s {:.2}; 20 m, at rest {:.2}", 2.0 * h + 1.0, at(2.0 * h + 1.0, 100.0), at(20.0, 0.0));
    assert!(low > 0.3, "turbulence at 20 m and 100 m/s: {low}");
    assert_eq!(at(2.0 * h + 1.0, 100.0), 0.0, "above twice the turbulence height: none");
    assert_eq!(at(20.0, 0.0), 0.0, "at rest: none");
    assert!(at(h * 1.5, 100.0) < low && at(h * 1.5, 100.0) > 0.0, "between the bands it fades");
}

#[test]
fn turbulence_is_repeatable_at_the_same_place_and_time() {
    let env = Air::default();
    let t = tuning().air;
    let run = || {
        let mut s = AirState::default();
        let mut b = body_at(DVec3::Y * 20.0);
        b.lin_vel = DVec3::new(0.0, 0.0, -60.0);
        let mut out = Vec::new();
        for _ in 0..30 {
            let f = Frame::new(&b, &env, DT);
            let o = air::step(&mut s, &f, &Modes::default(), &t);
            out.push((o.angular, o.push));
        }
        out
    };
    let (a, b) = (run(), run());
    assert_eq!(a, b, "same inputs, same outputs");
    let moving = a.iter().map(|(ang, _)| ang.length()).fold(0.0, f64::max);
    println!("largest angular turbulence over 0.5 s at 20 m: {moving:.4} rad/s²");
    assert!(moving > 0.0, "the air shakes the ship at 20 m");
}

#[test]
fn wind_is_a_base_wind_with_gusts_that_change_over_time() {
    let env = Air::default();
    let t = tuning().air;
    let mut b = body_at(DVec3::Y * 50.0);
    b.lin_vel = DVec3::ZERO;
    let f = Frame::new(&b, &env, DT);
    let mut s = AirState::default();
    let first = air::step(&mut s, &f, &Modes::default(), &t).wind;
    let mut later = first;
    for _ in 0..600 {
        later = air::step(&mut s, &f, &Modes::default(), &t).wind;
    }
    println!("wind at t=0 {:.2?}, at t=10 s {:.2?}", first, later);
    assert!((first - later).length() > 0.01, "the gusts move the wind");
    let base = t.wind_speed;
    let bound = base + t.gust_speed * 2.0_f64.sqrt() + 1e-9;
    assert!(first.length() <= bound && later.length() <= bound, "the wind stays within base plus gusts");
}

#[test]
fn space_is_all_zero_but_the_thrust_and_caps() {
    let env = Space::default();
    let t = tuning().air;
    let mut b = body_at(DVec3::new(0.0, 9000.0, 0.0));
    b.lin_vel = DVec3::new(30.0, 10.0, -100.0);
    b.ang_vel = DVec3::new(1.0, -2.0, 0.5);
    let f = Frame::new(&b, &env, DT);
    let mut s = AirState::default();
    let o = air::step(&mut s, &f, &Modes::default(), &t);
    assert_eq!(o.accel, DVec3::ZERO);
    assert_eq!(o.push, DVec3::ZERO);
    assert_eq!(o.angular, DVec3::ZERO);
    assert_eq!(o.wind, DVec3::ZERO);
    assert_eq!(o.turbulence, 0.0);
    assert_eq!((o.thrust_scale, o.cap_scale), (1.0, 1.0));
}

#[test]
fn broken_air_tuning_is_refused() {
    let base = tuning();
    let json = |edit: &dyn Fn(&mut flight_core::sc::air::AirTuning)| {
        let mut a = base.air.clone();
        edit(&mut a);
        a.validate()
    };
    assert!(json(&|a| a.drag.x = -1.0).unwrap_err().contains("drag"));
    assert!(json(&|a| a.lift_curve = vec![[0.0, 0.0]]).is_err(), "a curve needs two points");
    assert!(json(&|a| a.lift_curve = vec![[0.0, 0.0], [10.0, 1.0], [5.0, 0.5]]).is_err(), "degrees must rise");
    assert!(json(&|a| a.gust_period = 0.0).is_err());
    assert!(json(&|a| a.turbulence_height = 0.0).is_err());
    assert!(json(&|a| a.turbulence_speed_max = a.turbulence_speed_min).is_err());
    assert!(json(&|a| a.weathervane_ref_speed = 0.0).is_err());
}
