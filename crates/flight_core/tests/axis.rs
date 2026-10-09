//! Spike 13: the axis model next to the classic one. Limits per axis and direction, decay,
//! precision mode, G-safety, coupled and decoupled, on a planet fixture and in space at 60 Hz.
use flight_core::axis::{Dirs, G0};
use flight_core::{BodyState, Field, FlightInput, FlightModel, PlanetEnv, ShipController, AxisTuning};
use glam::{DQuat, DVec2, DVec3};

const DT: f64 = 1.0 / 60.0;
const AXIS: &str = include_str!("../../../content/tuning/ship_axis.json");

/// A flat-enough planet: radius 5000 m, no terrain, with or without gravity and air.
struct Planet {
    gravity: bool,
    air: bool,
    field: Field,
}

impl PlanetEnv for Planet {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world
    }
    fn radius(&self) -> f64 {
        5000.0
    }
    fn height_at(&self, _dir: DVec3) -> f64 {
        0.0
    }
    fn field(&self) -> &Field {
        &self.field
    }
    fn gravity_at(&self, world: DVec3) -> DVec3 {
        if self.gravity { -world.normalize() * 9.81 } else { DVec3::ZERO }
    }
    fn density_at(&self, _world: DVec3) -> f64 {
        if self.air { 1.0 } else { 0.0 }
    }
}

struct Sim {
    env: Planet,
    body: BodyState,
    ship: ShipController,
}

impl Sim {
    /// At `height` m above the ground, level, at rest, flying the axis model.
    fn new(height: f64, gravity: bool) -> Sim {
        let mut ship = ShipController::default();
        ship.set_model(FlightModel::Axis);
        ship.horizon_follow = false;
        ship.tuning.boost_capacitor.drain_time = 0.0;
        Sim { env: Planet { gravity, air: false, field: Field::default() }, body: BodyState { pos: DVec3::new(0.0, 5000.0 + height, 0.0), ..Default::default() }, ship }
    }

    fn step(&mut self, input: &FlightInput) {
        let (v, w) = self.ship.step(&self.body, input, &self.env, DT);
        self.body.lin_vel = v;
        self.body.ang_vel = w;
        self.body.integrate(DT);
    }

    /// Steps `secs` seconds; returns the largest acceleration between two steps (m/s²).
    fn run(&mut self, input: &FlightInput, secs: f64) -> f64 {
        let mut max: f64 = 0.0;
        for _ in 0..(secs / DT).round() as usize {
            let v0 = self.body.lin_vel;
            self.step(input);
            max = max.max((self.body.lin_vel - v0).length() / DT);
        }
        max
    }

    fn local_v(&self) -> DVec3 {
        self.body.rot.inverse() * self.body.lin_vel
    }
}

/// Scripted input (not piloted: no input ramp), as the scenarios' test drive.
fn scripted(thrust: DVec3) -> FlightInput {
    FlightInput { thrust, ..Default::default() }
}

#[test]
fn shipped_file_equals_default_and_bad_values_are_refused() {
    assert_eq!(AxisTuning::from_json(AXIS).unwrap(), AxisTuning::default());
    let bad = |from: &str, to: &str| {
        assert!(AXIS.contains(from), "fixture text {from:?} not in ship_axis.json");
        AxisTuning::from_json(&AXIS.replacen(from, to, 1)).unwrap_err()
    };
    assert!(bad("\"linear_decay\": 2.0", "\"linear_decay\": 0.0").contains("linear_decay"));
    assert!(bad("\"forward\": 30.0", "\"forward\": -1.0").contains("accel.forward"));
    assert!(bad("\"full_below\": 15.0, \"off_above\": 80.0", "\"full_below\": 90.0, \"off_above\": 80.0").contains("precision"));
    assert!(bad("\"cruise_speed\"", "\"cruise_sped\": 1.0, \"cruise_speed\"").contains("cruise_sped"));
    assert!(bad("\"boost_speed_backward\": 200.0", "\"boost_speed_backward\": 100.0").contains("below cruise_speed"));
}

#[test]
fn classic_by_default_and_the_switch_changes_the_response() {
    let mut s = Sim::new(500.0, false);
    s.ship.set_model(FlightModel::Classic);
    assert_eq!(ShipController::default().model, FlightModel::Classic);
    assert_eq!(FlightModel::Classic.next(), FlightModel::Axis);
    s.run(&scripted(DVec3::NEG_Z), 1.0);
    let classic = s.body.lin_vel.length();
    let mut a = Sim::new(500.0, false);
    a.run(&scripted(DVec3::NEG_Z), 1.0);
    let axis = a.body.lin_vel.length();
    assert!((classic - axis).abs() > 1.0, "the two models answer W differently after 1 s: classic {classic:.2}, axis {axis:.2} m/s");
}

#[test]
fn acceleration_is_limited_per_axis_and_direction() {
    let t = AxisTuning::default();
    for (name, stick, limit, cap) in [
        ("forward", DVec3::NEG_Z, t.accel.forward, t.cruise_speed),
        ("backward", DVec3::Z, t.accel.backward, t.cruise_speed),
        ("right", DVec3::X, t.accel.right, t.cruise_speed),
        ("down", DVec3::NEG_Y, t.accel.down, t.cruise_speed),
    ] {
        let mut s = Sim::new(5000.0, false);
        let max = s.run(&scripted(stick), 30.0);
        let along = s.local_v().dot(stick);
        assert!(max <= limit + 1e-6, "{name}: largest acceleration {max:.3} m/s², limit {limit}");
        assert!(max >= limit * 0.99, "{name}: far from the goal the limit is used ({max:.3} of {limit})");
        assert!((along - cap).abs() < 0.5, "{name}: reaches the cruise speed {cap} ({along:.2} m/s)");
    }
}

#[test]
fn decay_closes_the_last_metres_per_second_exponentially() {
    let t = AxisTuning::default();
    let mut s = Sim::new(5000.0, false);
    s.run(&scripted(DVec3::NEG_Z), 30.0);
    // Release: the error shrinks by exp(-decay * t) once it is below accel / decay.
    s.run(&scripted(DVec3::ZERO), (t.cruise_speed - t.accel.backward / t.linear_decay) / t.accel.backward);
    let v0 = s.body.lin_vel.length();
    assert!(v0 < t.accel.backward / t.linear_decay + 0.5, "saturated phase over: {v0:.2} m/s");
    s.run(&scripted(DVec3::ZERO), 1.0);
    let v1 = s.body.lin_vel.length();
    let want = v0 * (-t.linear_decay).exp();
    assert!((v1 - want).abs() < 0.05 * v0, "one second of decay: {v0:.3} -> {v1:.3} m/s, exp says {want:.3}");
}

#[test]
fn the_thrusters_hold_gravity_unless_a_side_is_too_weak() {
    // Level: hovers.
    let mut s = Sim::new(500.0, true);
    s.run(&scripted(DVec3::ZERO), 5.0);
    assert!((s.body.pos.y - 5500.0).abs() < 0.01, "level hover holds: {:.3} m", s.body.pos.y - 5000.0);
    // Rolled on its side: the side thrust (12 m/s²) still holds 1 g.
    let mut s = Sim::new(500.0, true);
    s.body.rot = DQuat::from_rotation_z(90f64.to_radians());
    s.run(&scripted(DVec3::ZERO), 5.0);
    assert!((s.body.pos.y - 5500.0).abs() < 0.01, "on its side with 12 m/s² sideways: {:.3} m", s.body.pos.y - 5000.0);
    // A side weaker than 1 g: the ship sinks at the missing acceleration.
    let mut s = Sim::new(500.0, true);
    s.ship.axis_tuning.accel.left = 8.0;
    s.ship.axis_tuning.accel.right = 8.0;
    s.body.rot = DQuat::from_rotation_z(90f64.to_radians());
    s.run(&scripted(DVec3::ZERO), 1.0);
    assert!(s.ship.axis.saturated);
    let sink = -s.body.lin_vel.y;
    assert!(sink > 0.5 && sink < 9.81 - 8.0 + 0.1, "with 8 m/s² sideways it sinks: {sink:.3} m/s after 1 s");
}

#[test]
fn assist_off_falls_and_decoupled_keeps_its_vector() {
    let mut s = Sim::new(500.0, true);
    s.ship.hover_assist = false;
    s.run(&scripted(DVec3::ZERO), 1.0);
    assert!((s.body.lin_vel.y + 9.81).abs() < 0.01, "assist off: free fall {:.3} m/s", s.body.lin_vel.y);

    let mut s = Sim::new(500.0, true);
    s.run(&scripted(DVec3::NEG_Z), 10.0);
    s.ship.coupled = false;
    s.ship.coupling = 0.0;
    let v0 = s.body.lin_vel;
    s.run(&scripted(DVec3::ZERO), 5.0);
    assert!((s.body.lin_vel - v0).length() < 1e-6, "decoupled with gravity compensation: {:?} -> {:?}", v0, s.body.lin_vel);
    // Thrust along the stick only, at the limit of that direction.
    s.run(&scripted(DVec3::X), 1.0);
    let side = (s.body.lin_vel - v0).x;
    assert!((side - s.ship.axis_tuning.accel.right).abs() < 0.01, "decoupled: 1 s of D gives {side:.3} m/s");
}

#[test]
fn precision_mode_caps_speed_near_the_ground_but_not_the_climb() {
    let t = AxisTuning::default();
    let mut s = Sim::new(5.0, true);
    s.run(&scripted(DVec3::NEG_Z), 10.0);
    // The ship moved sideways over a 5 km sphere: still in the band.
    assert!(s.ship.axis.precision > 0.99, "precision {:.3} at {:.1} m", s.ship.axis.precision, s.ship.terrain_clearance);
    // Along the ground: the straight flight over the sphere climbs a little, and the climb is free.
    let up = s.body.pos.normalize();
    let v = (s.body.lin_vel - up * s.body.lin_vel.dot(up)).length();
    assert!(v <= t.precision.speed + 0.05, "forward at 5 m: {v:.2} m/s along the ground, cap {}", t.precision.speed);
    let mut s = Sim::new(2.0, true);
    s.run(&scripted(DVec3::Y), 3.0);
    assert!(s.body.lin_vel.y > 20.0, "climbing away from the ground is not capped: {:.2} m/s", s.body.lin_vel.y);
    // From 400 m with full down stick: fast at first, then slowed early enough by the stopping
    // distance, down to the landing share of the cap.
    let mut s = Sim::new(400.0, true);
    let mut fastest: f64 = 0.0;
    while s.ship.terrain_clearance > 1.0 || s.body.pos.y > 5100.0 {
        s.step(&scripted(DVec3::NEG_Y));
        fastest = fastest.max(-s.body.lin_vel.y);
        let stop = s.body.lin_vel.y.powi(2) / (2.0 * (t.accel.up - 9.81));
        assert!(stop < s.ship.terrain_clearance + 2.0, "can still stop: {stop:.1} m needed at {:.1} m", s.ship.terrain_clearance);
    }
    let touch = -s.body.lin_vel.y;
    let want = t.precision.speed * t.precision.landing_share;
    assert!(fastest > 30.0 && (touch - want).abs() < 0.1, "descent from 400 m: fastest {fastest:.1} m/s, at 1 m {touch:.2} m/s (landing cap {want})");
}

#[test]
fn g_safety_caps_the_turn_at_speed_and_the_felt_g() {
    let t = AxisTuning::default();
    let yaw = FlightInput { thrust: DVec3::NEG_Z, turn: DVec2::new(0.0, 1.0), ..Default::default() };
    let mut s = Sim::new(5000.0, false);
    s.run(&scripted(DVec3::NEG_Z), 30.0);
    let mut felt: f64 = 0.0;
    for _ in 0..180 {
        s.step(&yaw);
        felt = felt.max(s.ship.axis.felt_g);
    }
    let rate = (s.body.rot.inverse() * s.body.ang_vel).y;
    // The cap of the last step: a left yaw pulls the forward velocity to the left.
    let speed = -s.local_v().z;
    let cap = t.g_safety.limit.left * G0 / speed;
    assert!(s.ship.axis.rate_capped && (rate - cap).abs() < 0.02 * cap, "yaw at {speed:.0} m/s: {rate:.3} rad/s, G cap {cap:.3}");
    assert!(felt <= t.g_safety.limit.forward + 1e-9, "felt {felt:.2} g");
    // Without it the nose turns at the full rate.
    let mut s = Sim::new(5000.0, false);
    s.ship.axis_tuning.g_safety.enabled = false;
    s.run(&scripted(DVec3::NEG_Z), 30.0);
    s.run(&yaw, 3.0);
    let rate = (s.body.rot.inverse() * s.body.ang_vel).y;
    assert!((rate - t.rate.yaw).abs() < 0.01, "no G-safety: {rate:.3} rad/s, cap {}", t.rate.yaw);
}

#[test]
fn turn_rates_have_an_acceleration_limit_and_an_ellipse() {
    let t = AxisTuning::default();
    let mut s = Sim::new(5000.0, false);
    let full = FlightInput { turn: DVec2::new(0.0, 1.0), ..Default::default() };
    s.run(&full, 0.1);
    let rate = (s.body.rot.inverse() * s.body.ang_vel).y;
    assert!(rate <= t.angular_accel.yaw * 0.1 + 1e-9 && rate > 0.0, "after 0.1 s: {rate:.3} rad/s");
    let mut s = Sim::new(5000.0, false);
    s.run(&FlightInput { turn: DVec2::new(1.0, 1.0), ..Default::default() }, 3.0);
    let w = s.body.rot.inverse() * s.body.ang_vel;
    let e = (w.x / t.rate.pitch).powi(2) + (w.y / t.rate.yaw).powi(2);
    assert!((e - 1.0).abs() < 0.01, "diagonal stick stays on the ellipse: pitch {:.3}, yaw {:.3}", w.x, w.y);
}

#[test]
fn dirs_clamp_and_along() {
    let d = Dirs { forward: 3.0, backward: 1.0, left: 2.0, right: 4.0, up: 5.0, down: 6.0 };
    assert_eq!(d.clamp(DVec3::new(-10.0, -10.0, -10.0)), DVec3::new(-2.0, -6.0, -3.0));
    assert_eq!(d.clamp(DVec3::new(10.0, 10.0, 10.0)), DVec3::new(4.0, 5.0, 1.0));
    assert_eq!(d.along(DVec3::new(-0.5, 1.0, -1.0)), DVec3::new(-1.0, 5.0, -3.0));
}

#[test]
fn switching_mid_flight_keeps_the_velocity() {
    // 10 s of W with each model, then the other one for one step: no jump past one step of
    // the largest acceleration either model asks for.
    for first in [FlightModel::Classic, FlightModel::Axis] {
        let mut s = Sim::new(500.0, true);
        s.ship.horizon_follow = true;
        s.ship.set_model(first);
        s.run(&scripted(DVec3::NEG_Z), 10.0);
        let v0 = s.body.lin_vel;
        s.ship.set_model(first.next());
        s.step(&scripted(DVec3::NEG_Z));
        let jump = (s.body.lin_vel - v0).length() / DT;
        assert!(jump < 80.0, "{first:?} -> {:?}: {jump:.1} m/s² in the switching step (at {:.1} m/s)", first.next(), v0.length());
    }
}

#[test]
fn g_safety_counts_gravity_and_the_direction_of_travel() {
    let t = AxisTuning::default();
    let pitch_up = |s: &mut Sim| {
        s.step(&FlightInput { turn: DVec2::new(1.0, 0.0), ..Default::default() });
        (s.body.rot.inverse() * s.body.ang_vel).x
    };
    // 150 m/s forward under 1 g: nose up needs thrust up on top of the 1 g hold: (6 - 1) g.
    // Backward, nose up pulls the velocity down: thrust down, which the hold relieves: (3 + 1) g.
    for (dir, limit) in [(DVec3::NEG_Z, t.g_safety.limit.up - 1.0), (DVec3::Z, t.g_safety.limit.down + 1.0)] {
        let mut s = Sim::new(500.0, true);
        s.body.lin_vel = dir * 150.0;
        let mut rate = 0.0;
        // Held level at 150 m/s: only the rate answers.
        for _ in 0..60 {
            s.body.lin_vel = dir * 150.0;
            s.body.rot = DQuat::IDENTITY;
            rate = pitch_up(&mut s);
        }
        let want = limit * G0 / 150.0;
        assert!(s.ship.axis.rate_capped && (rate - want).abs() < 0.01 * want, "{dir}: pitch {rate:.4} rad/s, want {want:.4}");
    }
}

#[test]
fn the_stopping_distance_counts_the_attitude() {
    // Rolled on its side the up thrust is the side thrust (12 m/s², 2.2 m/s² after gravity), so
    // a 30 m/s descent at 200 m (205 m to stop) is already deep in the band; level (15.2 m/s²,
    // 30 m to stop) it is not.
    let mut level = Sim::new(200.0, true);
    level.body.lin_vel = DVec3::new(0.0, -30.0, 0.0);
    level.step(&scripted(DVec3::ZERO));
    let mut rolled = Sim::new(200.0, true);
    rolled.body.rot = DQuat::from_rotation_z(90f64.to_radians());
    rolled.body.lin_vel = DVec3::new(0.0, -30.0, 0.0);
    rolled.step(&scripted(DVec3::ZERO));
    assert!(level.ship.axis.precision < 0.01 && rolled.ship.axis.precision > 0.99, "precision level {:.3}, rolled {:.3}", level.ship.axis.precision, rolled.ship.axis.precision);
}

#[test]
fn decoupled_on_the_ground_holds_still() {
    let mut s = Sim::new(0.0, true);
    s.ship.coupled = false;
    s.ship.coupling = 0.0;
    s.body.lin_vel = DVec3::new(3.0, 0.0, 0.0);
    s.step(&FlightInput { grounded: true, ..Default::default() });
    let side = s.body.lin_vel.x.abs();
    assert!(side < 1e-9, "no sideways speed while holding on the ground: {side:.4} m/s");
}

#[test]
fn switching_drops_the_classic_ground_hold() {
    let mut s = ShipController::default();
    s.ground_hold = Some(flight_core::GroundHold { at: DVec3::ZERO, rest: Some(DVec3::ZERO) });
    s.set_model(FlightModel::Axis);
    s.set_model(FlightModel::Classic);
    assert_eq!(s.ground_hold, None);
}
