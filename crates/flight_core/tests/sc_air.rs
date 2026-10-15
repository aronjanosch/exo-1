//! Numbers are printed (`cargo test -p flight_core sc_air -- --nocapture`).
mod sc_common;

use flight_core::sc::air::{self, AirState, Axes};
use flight_core::sc::{StepState, Modes};
use glam::DVec3;
use sc_common::*;

/// The air stage on a body at `pos` moving with `vel` (world), in an air planet with no gravity.
fn air_at(pos: DVec3, vel: DVec3, env: &Air, t: &flight_core::sc::air::AirTuning) -> air::AirOut {
    let mut b = body_at(pos);
    b.lin_vel = vel;
    let f = StepState::new(&b, env, DT);
    air::step(&mut AirState::default(), &f, &Modes::default(), t)
}

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
fn weathervaning_turns_the_nose_towards_the_airspeed() {
    let env = no_gravity();
    let t = quiet();
    // Moving right of the nose (airspeed to +x): the nose must turn right, a negative yaw rate.
    let right = air_at(DVec3::Y * 100.0, DVec3::new(10.0, 0.0, -50.0), &env, &t).angular_hold;
    // Moving up of the nose (airspeed to +y): the nose must come up, a positive pitch rate.
    let up = air_at(DVec3::Y * 100.0, DVec3::new(0.0, 10.0, -50.0), &env, &t).angular_hold;
    let straight = air_at(DVec3::Y * 100.0, DVec3::new(0.0, 0.0, -50.0), &env, &t).angular_hold;
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
    let f = StepState::new(&b, &env, DT);
    let on = air::step(&mut AirState::default(), &f, &Modes { wind_comp: true, ..Modes::default() }, &t);
    let off = air::step(&mut AirState::default(), &f, &Modes { wind_comp: false, ..Modes::default() }, &t);
    println!("wind {:.2} m/s at rest: held {:.3} m/s², pushed {:.3} m/s²", on.wind.length(), on.accel.length(), off.push.length());
    assert!(on.wind.length() > 5.0, "the 10 m/s base wind (gusts aside): {:?}", on.wind);
    assert!(on.push == DVec3::ZERO, "with wind compensation nothing is pushed");
    let expected = DVec3::new(t.drag.x * on.wind.x * on.wind.x.abs(), t.drag.y * on.wind.y * on.wind.y.abs(), t.drag.z * on.wind.z * on.wind.z.abs()) * env.density;
    assert!((on.accel - expected).length() < 1e-12 && expected.length() > 0.0, "the weak wind force is held: {:?}, expected {expected:?}", on.accel);
    assert!(off.accel == DVec3::ZERO, "without wind compensation the wind is not held");
    assert!((off.push - on.accel).length() < 1e-9, "the same force, on the other list");
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
            let f = StepState::new(&b, &env, DT);
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
fn space_is_all_zero_but_the_thrust_and_caps() {
    let env = Space::default();
    let t = tuning().air;
    let mut b = body_at(DVec3::new(0.0, 9000.0, 0.0));
    b.lin_vel = DVec3::new(30.0, 10.0, -100.0);
    b.ang_vel = DVec3::new(1.0, -2.0, 0.5);
    let f = StepState::new(&b, &env, DT);
    let mut s = AirState::default();
    let o = air::step(&mut s, &f, &Modes::default(), &t);
    assert_eq!(o.accel, DVec3::ZERO);
    assert_eq!(o.push, DVec3::ZERO);
    assert_eq!(o.angular, DVec3::ZERO);
    assert_eq!(o.angular_hold, DVec3::ZERO);
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
