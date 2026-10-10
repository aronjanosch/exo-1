//! #184: thrust direction and the brake, numbers only. Today's law (the shipped
//! `ShipController::step`) next to a candidate on the same thrust box, in space at 60 Hz with the
//! placeholders in `ship.json`. Nothing here changes how the game flies; the candidate lives only
//! in this file. `cargo test -p flight_core --test thrust_compare -- --nocapture` prints the table.
use flight_core::axis::{Dirs, G0};
use flight_core::{BodyState, Field, FlightInput, PlanetEnv, ShipController, ShipTuning};
use glam::DVec3;

const DT: f64 = 1.0 / 60.0;
const SHIP: &str = include_str!("../../../content/tuning/ship.json");
/// s: a manoeuvre that has not finished by then counts as not finished.
const MAX_TIME: f64 = 60.0;

/// Deep space: no gravity, no air, far above a small planet.
struct Space {
    field: Field,
}

impl PlanetEnv for Space {
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
    fn gravity_at(&self, _world: DVec3) -> DVec3 {
        DVec3::ZERO
    }
    fn density_at(&self, _world: DVec3) -> f64 {
        0.0
    }
}

const START: DVec3 = DVec3::new(0.0, 1.0e6, 0.0);

fn tuning() -> ShipTuning {
    ShipTuning::from_json(SHIP).unwrap()
}

/// The box `step` clamps to in space, assisted, no boost: thrust limits inside the G tolerance.
fn assist_box(t: &ShipTuning) -> Dirs {
    t.accel.min(&t.g_safety.limit.scaled(G0))
}

/// The brake's box: the brake always gets the boost multipliers.
fn brake_box(t: &ShipTuning) -> Dirs {
    t.accel.mul(&t.boost_accel).min(&t.g_safety.limit.scaled(G0))
}

/// The most acceleration along the unit vector `u` that still fits the box: the weakest axis in
/// that direction (its limit over how much of `u` it has to give).
fn reach(b: &Dirs, u: DVec3) -> f64 {
    let lim = b.along(u.signum());
    let mut r = f64::INFINITY;
    for (l, c) in [(lim.x, u.x), (lim.y, u.y), (lim.z, u.z)] {
        if c.abs() > 1e-12 {
            r = r.min(l.abs() / c.abs());
        }
    }
    r
}

/// Today, as a formula: the per-axis clamp of `(goal − v) × decay`. Checked against `step` below.
fn today(b: &Dirs, decay: f64, v: DVec3, goal: DVec3) -> DVec3 {
    b.clamp((goal - v) * decay)
}

/// Candidate, assisted with a goal (a reading of the anti-drift law, TODO(initiator)): while the
/// request sticks out of the box and a desired direction exists, the component of the error
/// across that direction gets the full thrust the box has that way, and the component along it
/// is paced to finish no earlier than the cross component; inside the box it is today's law.
fn candidate(b: &Dirs, decay: f64, v: DVec3, goal: DVec3) -> DVec3 {
    let asked = (goal - v) * decay;
    let d = goal.normalize_or_zero();
    if (b.clamp(asked) - asked).length() < 1e-9 || d == DVec3::ZERO {
        return today(b, decay, v, goal);
    }
    let e = goal - v;
    let along = e.dot(d);
    let cross = e - d * along;
    if cross.length() < 1e-9 {
        let u = e.normalize();
        return u * reach(b, u).min(e.length() * decay);
    }
    let cu = cross.normalize();
    let c_reach = reach(b, cu);
    let c = cu * c_reach.min(cross.length() * decay);
    // Along: what is left in the box next to `c`, paced so the cross component is gone first.
    let au = d * along.signum();
    let paced = along.abs() / (cross.length() / c_reach);
    let (mut lo, mut hi) = (0.0, paced.min(along.abs() * decay));
    if (b.clamp(c + au * hi) - (c + au * hi)).length() < 1e-9 {
        lo = hi;
    }
    for _ in 0..60 {
        let k = 0.5 * (lo + hi);
        if (b.clamp(c + au * k) - (c + au * k)).length() < 1e-9 {
            lo = k;
        } else {
            hi = k;
        }
    }
    c + au * lo
}

/// Candidate brake: one deceleration along the actual velocity, capped by the weakest axis in
/// that direction, proportional to the speed near zero (`decay` per second).
fn candidate_brake(b: &Dirs, decay: f64, v: DVec3) -> DVec3 {
    let s = v.length();
    if s < 1e-9 {
        return DVec3::ZERO;
    }
    let u = -v / s;
    u * reach(b, u).min(s * decay)
}

#[derive(Debug)]
struct Run {
    /// s until the velocity is within `done` of the goal; `None` if not by `MAX_TIME`.
    time: Option<f64>,
    /// m flown.
    path: f64,
    /// m/s: the highest speed on the way.
    peak: f64,
    /// m/s: the farthest the velocity strays from the straight line from start to goal.
    off_line: f64,
    /// Degrees: the farthest the velocity heading turns away from the start heading (stops only).
    heading: f64,
}

/// Steps `accel(v)` from `v0` towards `goal` (semi-implicit Euler, as `step` and the body do).
fn fly(v0: DVec3, goal: DVec3, done: f64, mut accel: impl FnMut(DVec3) -> DVec3) -> Run {
    let mut v = v0;
    let mut run = Run { time: None, path: 0.0, peak: v0.length(), off_line: 0.0, heading: 0.0 };
    let line = goal - v0;
    for i in 1..=(MAX_TIME / DT) as usize {
        v += accel(v) * DT;
        run.path += v.length() * DT;
        run.peak = run.peak.max(v.length());
        let s = ((v - v0).dot(line) / line.length_squared()).clamp(0.0, 1.0);
        run.off_line = run.off_line.max((v - (v0 + line * s)).length());
        if goal == DVec3::ZERO && v.length() > 1.0 {
            run.heading = run.heading.max(v.angle_between(v0).to_degrees());
        }
        if (v - goal).length() < done {
            run.time = Some(i as f64 * DT);
            break;
        }
    }
    run
}

/// The shipped step from `v0` under `input` (level ship, coupled, assisted, in space), with the
/// F7 switches `law` = (A3 cap refuses thrust, B2 brake keeps the heading).
fn fly_step(v0: DVec3, goal: DVec3, done: f64, input: FlightInput, law: (bool, bool)) -> Run {
    let mut ship = ShipController::new(tuning());
    ship.horizon_follow = false;
    ship.tuning.boost_capacitor.drain_time = 0.0;
    ship.cap_refuses_thrust = law.0;
    ship.brake_keeps_heading = law.1;
    let env = Space { field: Field::default() };
    let mut body = BodyState { pos: START, lin_vel: v0, ..Default::default() };
    fly(v0, goal, done, |v| {
        body.lin_vel = v;
        let (nv, w) = ship.step(&body, &input, &env, DT);
        body.lin_vel = nv;
        body.ang_vel = w;
        body.integrate(DT);
        (nv - v) / DT
    })
}

/// The shipped step with both F7 switches on.
fn f7(t: &ShipTuning, c: &Case) -> Run {
    fly_step(c.v0, goal(t, c), DONE, input(c), (true, true))
}

struct Case {
    name: &'static str,
    v0: DVec3,
    stick: DVec3,
    brake: bool,
}

fn cases(t: &ShipTuning) -> Vec<Case> {
    let cruise = t.space.cruise_speed;
    let diag = DVec3::new(1.0, 0.0, -1.0).normalize() * cruise;
    vec![
        Case { name: "1 sideways at the cap, then full forward", v0: DVec3::X * cruise, stick: DVec3::NEG_Z, brake: false },
        Case { name: "2a stop from cruise along the nose, brake", v0: DVec3::NEG_Z * cruise, stick: DVec3::ZERO, brake: true },
        Case { name: "2b stop from cruise 45° forward-right, brake", v0: diag, stick: DVec3::ZERO, brake: true },
        Case { name: "3 stop from 45° forward-right, brake + right held", v0: diag, stick: DVec3::X, brake: true },
    ]
}

/// The goal the stick asks for in space (cruise cap in every direction, no boost).
fn goal(t: &ShipTuning, c: &Case) -> DVec3 {
    if c.brake { DVec3::ZERO } else { c.stick * t.space.cruise_speed }
}

fn input(c: &Case) -> FlightInput {
    // The brake only counts while piloted; piloted input is ramped, which the brake's zero stick
    // does not see. Without the brake: scripted, no ramp.
    FlightInput { thrust: c.stick, brake: c.brake, piloted: c.brake, ..Default::default() }
}

const DONE: f64 = 0.1;

fn runs(t: &ShipTuning, c: &Case) -> (Run, Run) {
    let g = goal(t, c);
    let shipped = fly_step(c.v0, g, DONE, input(c), (false, false));
    let (ab, bb, k) = (assist_box(t), brake_box(t), t.linear_decay);
    let cand = if c.brake { fly(c.v0, g, DONE, |v| candidate_brake(&bb, k, v)) } else { fly(c.v0, g, DONE, |v| candidate(&ab, k, v, g)) };
    (shipped, cand)
}

/// The formula `today` is what `step` does on these manoeuvres, so the candidate runs on the same
/// box as the shipped step.
#[test]
fn today_formula_matches_the_shipped_step() {
    let t = tuning();
    for c in cases(&t) {
        let g = goal(&t, &c);
        let b = if c.brake { brake_box(&t) } else { assist_box(&t) };
        let formula = fly(c.v0, g, DONE, |v| today(&b, t.linear_decay, v, g));
        let shipped = fly_step(c.v0, g, DONE, input(&c), (false, false));
        assert_eq!(formula.time, shipped.time, "{}", c.name);
        assert!((formula.path - shipped.path).abs() < 1e-6, "{}: {} vs {}", c.name, formula.path, shipped.path);
    }
}

#[test]
fn comparison_table() {
    let t = tuning();
    let cap = t.space.cruise_speed;
    let fmt_t = |r: &Run| r.time.map_or("not done".to_string(), |s| format!("{s:.2} s"));
    let over = |r: &Run| if r.peak > cap + 0.5 { format!("yes, {:.0} m/s", r.peak) } else { format!("no, {:.0} m/s", r.peak) };
    println!("\nIn space, cap {cap} m/s, assist box {:?}, brake box {:?}, done within {DONE} m/s\n", assist_box(&t), brake_box(&t));
    println!("| Manoeuvre | Law | Time | Path | Over the cap (peak) | Off the straight line | Heading turn |");
    println!("|---|---|---|---|---|---|---|");
    for c in cases(&t) {
        let (shipped, cand) = runs(&t, &c);
        let on = f7(&t, &c);
        for (law, r) in [("today", &shipped), ("candidate", &cand), ("F7 (A3 + B2)", &on)] {
            let heading = if c.brake { format!("{:.1}°", r.heading) } else { "–".to_string() };
            println!("| {} | {law} | {} | {:.0} m | {} | {:.1} m/s | {heading} |", c.name, fmt_t(r), r.path, over(r), r.off_line);
        }
        assert!(shipped.time.is_some() && cand.time.is_some(), "{}: not finished", c.name);
    }
}

/// What the candidate is for, as checks: no axis past the cap in manoeuvre 1, the heading kept on
/// every stop.
#[test]
fn candidate_keeps_the_cap_and_the_heading() {
    let t = tuning();
    for c in cases(&t) {
        let (_, cand) = runs(&t, &c);
        assert!(cand.peak <= t.space.cruise_speed + 0.5, "{}: peak {}", c.name, cand.peak);
        if c.brake {
            assert!(cand.heading < 0.5, "{}: heading turned {}°", c.name, cand.heading);
        }
    }
}

/// F7 on (A3 + B2) on manoeuvre 1: the velocity never gets faster than the cap.
#[test]
fn f7_refuses_thrust_past_the_cap() {
    let t = tuning();
    let c = &cases(&t)[0];
    let r = f7(&t, c);
    assert!(r.time.is_some(), "{}: not finished", c.name);
    assert!(r.peak <= t.space.cruise_speed + 0.5, "{}: peak {}", c.name, r.peak);
}

/// F7 on: every stop keeps its heading, and stop 2a (along the nose) is today's stop.
#[test]
fn f7_brake_keeps_the_heading_and_stop_2a_is_today() {
    let t = tuning();
    for c in cases(&t).iter().filter(|c| c.brake) {
        let r = f7(&t, c);
        assert!(r.time.is_some(), "{}: not finished", c.name);
        assert!(r.heading < 0.5, "{}: heading turned {}°", c.name, r.heading);
    }
    let c = &cases(&t)[1];
    let (today, on) = (fly_step(c.v0, goal(&t, c), DONE, input(c), (false, false)), f7(&t, c));
    assert_eq!(today.time, on.time, "{}", c.name);
    assert!((today.path - on.path).abs() < 1.0, "{}: {} vs {}", c.name, today.path, on.path);
}

/// F7 on, in full air with gravity: a diagonal brake from 100 m/s, 400 m up, stops within 10 s,
/// keeps its heading and its height along the start's up axis (not the distance to the centre:
/// the straight stop of about 230 m raises that by about 5 m through curvature alone).
#[test]
fn f7_brake_in_air_stops_on_its_line_at_its_height() {
    let env = Air { field: Field::default() };
    let up = DVec3::Y;
    let start = up * (5000.0 + 400.0);
    let v0 = DVec3::new(1.0, 0.0, -1.0).normalize() * 100.0;
    let mut ship = ShipController::new(tuning());
    ship.horizon_follow = false;
    ship.tuning.boost_capacitor.drain_time = 0.0;
    ship.cap_refuses_thrust = true;
    ship.brake_keeps_heading = true;
    let mut body = BodyState { pos: start, lin_vel: v0, ..Default::default() };
    let brake = FlightInput { brake: true, piloted: true, ..Default::default() };
    let (mut stop, mut heading, mut height) = (None, 0.0_f64, 0.0_f64);
    for i in 1..=(10.0 / DT) as usize {
        let (v, w) = ship.step(&body, &brake, &env, DT);
        body.lin_vel = v;
        body.ang_vel = w;
        body.integrate(DT);
        if v.length() > 1.0 {
            heading = heading.max(v.angle_between(v0).to_degrees());
        }
        height = height.max((body.pos.dot(up) - start.dot(up)).abs());
        if v.length() < 0.5 && stop.is_none() {
            stop = Some(i as f64 * DT);
        }
    }
    println!("air F7 brake: stop {stop:?} s, heading {heading:.3}°, height {height:.3} m");
    assert!(stop.is_some(), "not stopped within 10 s, speed {}", body.lin_vel.length());
    assert!(heading < 0.5, "heading turned {heading}°");
    assert!(height < 1.0, "height moved {height} m");
}

/// Gravity pulls down the planet's centre; drag as in the shipped step (full air).
struct Air {
    field: Field,
}

impl PlanetEnv for Air {
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
        -world.normalize() * 9.81
    }
    fn density_at(&self, _world: DVec3) -> f64 {
        1.0
    }
}
