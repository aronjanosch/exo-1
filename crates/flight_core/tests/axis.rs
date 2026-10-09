//! The axis flight model (spike 13): limits per axis and direction, decay, precision mode,
//! G-safety, coupled and decoupled, on a planet fixture and in space at 60 Hz.
use flight_core::axis::{Dirs, G0};
use flight_core::{BodyState, Field, FlightInput, PlanetEnv, ShipController, ShipTuning};
use glam::{DQuat, DVec2, DVec3};

const DT: f64 = 1.0 / 60.0;
const SHIP: &str = include_str!("../../../content/tuning/ship.json");

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
    /// At `height` m above the ground, level, at rest.
    fn new(height: f64, gravity: bool) -> Sim {
        let mut ship = ShipController::default();
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
    assert_eq!(ShipTuning::from_json(SHIP).unwrap(), ShipTuning::default());
    let bad = |from: &str, to: &str| {
        assert!(SHIP.contains(from), "fixture text {from:?} not in ship.json");
        ShipTuning::from_json(&SHIP.replacen(from, to, 1)).unwrap_err()
    };
    assert!(bad("\"linear_decay\": 3.0", "\"linear_decay\": 0.0").contains("linear_decay"));
    assert!(bad("\"atmosphere_thrust\": 0.5", "\"atmosphere_thrust\": 1.5").contains("atmosphere_thrust"));
    assert!(bad("\"forward\": 60.0", "\"forward\": -1.0").contains("accel.forward"));
    assert!(bad("\"full_below\": 5.0, \"off_above\": 40.0", "\"full_below\": 50.0, \"off_above\": 40.0").contains("precision"));
    assert!(bad("\"cruise_speed\"", "\"cruise_sped\": 1.0, \"cruise_speed\"").contains("cruise_sped"));
    assert!(bad("\"boost_speed_backward\": 200.0", "\"boost_speed_backward\": 100.0").contains("below cruise_speed"));
    // #106 point 5: NaN passed the `<` comparisons of the space caps.
    let mut t = ShipTuning::default();
    t.space.boost_speed_forward = f64::NAN;
    assert!(t.validate().unwrap_err().contains("space.boost_speed_forward"));
    let mut t = ShipTuning::default();
    t.boost_speed_backward = f64::INFINITY;
    assert!(t.validate().unwrap_err().contains("boost_speed_backward"));
    // Turn rates over speed: a share at or below 0 flips or zeroes the rates (clamp panics, NaN).
    for y in [0.0, -0.5] {
        let mut t = ShipTuning::default();
        t.rate_over_speed.points[1].y = y;
        assert!(t.validate().unwrap_err().contains("rate_over_speed"), "{y}");
    }
}

#[test]
fn acceleration_is_limited_per_axis_and_direction() {
    // In vacuum: the full thrust (inside the pilot's G tolerance) and the space caps.
    let t = ShipTuning::default();
    let (cap, g) = (t.space.cruise_speed, t.g_safety.limit.scaled(G0));
    for (name, stick, limit) in [("forward", DVec3::NEG_Z, t.accel.forward.min(g.forward)), ("backward", DVec3::Z, t.accel.backward.min(g.backward)), ("right", DVec3::X, t.accel.right.min(g.right)), ("down", DVec3::NEG_Y, t.accel.down.min(g.down))] {
        // High enough that 30 s down at the space cap stay far above the ground.
        let mut s = Sim::new(50000.0, false);
        let max = s.run(&scripted(stick), 30.0);
        let along = s.local_v().dot(stick);
        assert!(max <= limit + 1e-6, "{name}: largest acceleration {max:.3} m/s², limit {limit}");
        assert!(max >= limit * 0.99, "{name}: far from the goal the limit is used ({max:.3} of {limit})");
        assert!((along - cap).abs() < 0.5, "{name}: reaches the cruise speed {cap} ({along:.2} m/s)");
    }
}

#[test]
fn decay_closes_the_last_metres_per_second_exponentially() {
    let t = ShipTuning::default();
    let mut s = Sim::new(5000.0, false);
    s.run(&scripted(DVec3::NEG_Z), 30.0);
    // Release: the error shrinks by (1 - decay * dt) per step once it is below accel / decay
    // (the backward thrust inside the pilot's tolerance).
    let back = t.accel.backward.min(t.g_safety.limit.backward * G0);
    s.run(&scripted(DVec3::ZERO), (t.space.cruise_speed - back / t.linear_decay) / back + 0.1);
    let v0 = s.body.lin_vel.length();
    assert!(v0 < back / t.linear_decay + 0.5, "saturated phase over: {v0:.2} m/s");
    s.run(&scripted(DVec3::ZERO), 1.0);
    let v1 = s.body.lin_vel.length();
    let want = v0 * (1.0 - t.linear_decay * DT).powi(60);
    assert!((v1 - want).abs() < 0.02 * v0, "one second of decay: {v0:.3} -> {v1:.3} m/s, wanted {want:.3}");
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
    s.ship.tuning.accel.left = 8.0;
    s.ship.tuning.accel.right = 8.0;
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
    assert_eq!(s.ship.forward_speed_limit, 0.0, "no limit shown with the assist off");

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
    assert!((side - s.ship.tuning.accel.right).abs() < 0.01, "decoupled: 1 s of D gives {side:.3} m/s");
}

#[test]
fn precision_mode_caps_speed_near_the_ground_but_not_the_climb() {
    let t = ShipTuning::default();
    // Without landing mode, low flight is not capped.
    let mut s = Sim::new(2.0, true);
    s.run(&scripted(DVec3::NEG_Z), 4.0);
    assert!(s.ship.axis.precision == 0.0 && s.body.lin_vel.length() > 2.0 * t.precision.speed, "no landing mode: {:.1} m/s at 2 m", s.body.lin_vel.length());
    let mut s = Sim::new(2.0, true);
    s.ship.landing_mode = true;
    s.ship.horizon_follow = true;
    s.run(&scripted(DVec3::NEG_Z), 4.0);
    // The ship moved sideways over a 5 km sphere: still in the band.
    assert!(s.ship.axis.precision > 0.99, "precision {:.3} at {:.1} m", s.ship.axis.precision, s.ship.terrain_clearance);
    // Along the ground: the straight flight over the sphere climbs a little, and the climb is free.
    let up = s.body.pos.normalize();
    let v = (s.body.lin_vel - up * s.body.lin_vel.dot(up)).length();
    assert!(v <= t.precision.speed + 0.05, "forward at 2 m: {v:.2} m/s along the ground, cap {}", t.precision.speed);
    let mut s = Sim::new(2.0, true);
    s.ship.landing_mode = true;
    s.run(&scripted(DVec3::Y), 3.0);
    assert!(s.body.lin_vel.y > 20.0, "climbing away from the ground is not capped: {:.2} m/s", s.body.lin_vel.y);
    // From 400 m with full down stick, landing mode or not: fast at first, then slowed early
    // enough by the stopping distance, down to the landing share of the cap.
    for landing in [true, false] {
    let mut s = Sim::new(400.0, true);
    s.ship.landing_mode = landing;
    let mut fastest: f64 = 0.0;
    while s.ship.terrain_clearance > 0.05 || s.body.pos.y > 5100.0 {
        s.step(&scripted(DVec3::NEG_Y));
        fastest = fastest.max(-s.body.lin_vel.y);
        let stop = s.body.lin_vel.y.powi(2) / (2.0 * (t.accel.up - 9.81));
        assert!(stop < s.ship.terrain_clearance + 2.0, "can still stop: {stop:.1} m needed at {:.1} m", s.ship.terrain_clearance);
    }
    let touch = -s.body.lin_vel.y;
    let want = t.precision.speed * t.precision.landing_share;
    assert!(fastest > 30.0 && touch <= want + 0.2 && touch >= 0.7 * want, "descent from 400 m (landing mode {landing}): fastest {fastest:.1} m/s, at the ground {touch:.2} m/s (landing cap {want})");
    }
}

#[test]
fn g_safety_caps_the_turn_at_speed_and_the_felt_g() {
    let t = ShipTuning::default();
    let yaw = FlightInput { thrust: DVec3::NEG_Z, turn: DVec2::new(0.0, 1.0), ..Default::default() };
    let mut s = Sim::new(5000.0, false);
    s.ship.tuning.g_safety.cap_turns = true;
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
    // Without the turn cap the nose turns at its rate over the speed.
    let mut s = Sim::new(5000.0, false);
    s.ship.tuning.g_safety.cap_turns = false;
    s.run(&scripted(DVec3::NEG_Z), 30.0);
    let mut felt: f64 = 0.0;
    for _ in 0..180 {
        s.step(&yaw);
        felt = felt.max(s.ship.axis.felt_g);
    }
    let rate = (s.body.rot.inverse() * s.body.ang_vel).y;
    let want = t.rate.yaw * t.rate_over_speed.eval(s.body.lin_vel.length() / t.space.cruise_speed);
    assert!(!s.ship.axis.rate_capped && (rate - want).abs() < 0.03 * want, "no turn cap: {rate:.3} rad/s, want {want:.3}");
    let most = [t.g_safety.limit.forward, t.g_safety.limit.backward, t.g_safety.limit.left, t.g_safety.limit.right].into_iter().fold(0.0, f64::max);
    assert!(felt <= most.hypot(most) + 1e-9, "the thrust stays inside the tolerance box: {felt:.2} g");
}

#[test]
fn turn_rates_have_an_acceleration_limit_and_an_ellipse() {
    let t = ShipTuning::default();
    let mut s = Sim::new(5000.0, false);
    let full = FlightInput { turn: DVec2::new(0.0, 1.0), ..Default::default() };
    s.run(&full, 0.1);
    let rate = (s.body.rot.inverse() * s.body.ang_vel).y;
    assert!(rate <= t.angular_accel.yaw * 0.1 + 1e-9 && rate > 0.0, "after 0.1 s: {rate:.3} rad/s");
    let mut s = Sim::new(5000.0, false);
    s.run(&FlightInput { turn: DVec2::new(1.0, 1.0), ..Default::default() }, 3.0);
    let w = s.body.rot.inverse() * s.body.ang_vel;
    // At rest the rates are their share over the speed at 0.
    let share = t.rate_over_speed.eval(0.0);
    let e = (w.x / (t.rate.pitch * share)).powi(2) + (w.y / (t.rate.yaw * share)).powi(2);
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
fn g_safety_counts_gravity_and_the_direction_of_travel() {
    let t = ShipTuning::default();
    let pitch_up = |s: &mut Sim| {
        s.step(&FlightInput { turn: DVec2::new(1.0, 0.0), ..Default::default() });
        (s.body.rot.inverse() * s.body.ang_vel).x
    };
    // 150 m/s forward under 1 g: nose up needs thrust up on top of the 1 g hold: (6 - 1) g.
    // Backward, nose up pulls the velocity down: thrust down, which the hold relieves: (3 + 1) g.
    for (dir, limit) in [(DVec3::NEG_Z, t.g_safety.limit.up - 1.0), (DVec3::Z, t.g_safety.limit.down + 1.0)] {
        let mut s = Sim::new(500.0, true);
        s.ship.tuning.g_safety.cap_turns = true;
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
    // In full atmosphere: half the vacuum thrust.
    let mut level = Sim::new(200.0, true);
    level.env.air = true;
    level.ship.landing_mode = true;
    level.body.lin_vel = DVec3::new(0.0, -30.0, 0.0);
    level.step(&scripted(DVec3::ZERO));
    let mut rolled = Sim::new(200.0, true);
    rolled.env.air = true;
    rolled.ship.landing_mode = true;
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
fn decoupled_thrust_stops_at_the_caps() {
    let t = ShipTuning::default();
    let mut s = Sim::new(5000.0, false);
    s.ship.coupled = false;
    s.ship.coupling = 0.0;
    s.run(&scripted(DVec3::NEG_Z), 20.0);
    let v = s.body.lin_vel.length();
    assert!(v <= t.space.cruise_speed + t.accel.forward * DT + 1e-6 && v > 0.99 * t.space.cruise_speed, "decoupled W for 20 s: {v:.1} m/s, cap {}", t.space.cruise_speed);
    // The brake (X) still damps decoupled.
    s.run(&FlightInput { brake: true, piloted: true, ..Default::default() }, 10.0);
    assert!(s.body.lin_vel.length() < 0.5, "X stops the decoupled ship: {:.2} m/s", s.body.lin_vel.length());
}

#[test]
fn space_is_faster_and_the_thrust_stronger_than_in_air() {
    let t = ShipTuning::default();
    let top = |air: bool| {
        let mut s = Sim::new(5000.0, false);
        s.env.air = air;
        let max = s.run(&scripted(DVec3::NEG_Z), 0.5);
        s.run(&scripted(DVec3::NEG_Z), 30.0);
        (max, s.body.lin_vel.length())
    };
    let ((a_space, v_space), (a_air, v_air)) = (top(false), top(true));
    assert!((a_space - t.accel.forward).abs() < 1e-6 && (a_air - t.accel.forward * t.atmosphere_thrust).abs() < 0.5, "thrust: space {a_space:.2}, air {a_air:.2} m/s²");
    assert!((v_space - t.space.cruise_speed).abs() < 1.0 && (v_air - t.cruise_speed).abs() < 1.0, "cruise: space {v_space:.1}, air {v_air:.1} m/s");
}

/// #144 (`full`): Ctrl on a hull pitched a few degrees asks for the cruise speed along the ship's
/// down, part of it sideways; while the descent limit holds, the down input points straight down,
/// so the ship comes down without drifting (it touched down at 7.4 m/s sideways and slid 0.74 m).
/// At the start the thrusters saturate and give a short sideways push; it is gone low down.
#[test]
fn a_tilted_descent_does_not_drift_sideways() {
    let mut s = Sim::new(60.0, true);
    let mut low: f64 = 0.0;
    while s.ship.terrain_clearance > 0.5 || s.body.pos.y > 5030.0 {
        s.body.rot = DQuat::from_rotation_x(5f64.to_radians());
        s.body.ang_vel = DVec3::ZERO;
        s.step(&scripted(DVec3::NEG_Y));
        if s.ship.terrain_clearance < 10.0 {
            low = low.max(s.body.lin_vel.z.hypot(s.body.lin_vel.x));
        }
    }
    assert!(low < 0.1, "sideways at most {low:.3} m/s in the last 10 m");
}
