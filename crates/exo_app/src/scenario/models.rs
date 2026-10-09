//! Scenario `flight-models` (spike 13): the same manoeuvres with the classic and the axis model,
//! each from the same pose 400 m above the ground (the landing from 100 m), driven through
//! `Controls`; F7 switches the model through the bindings. Prints the numbers side by side.
use crate::controls::Controls;
use crate::env::PlanetRes;
use crate::hud::HudReadout;
use crate::scenario::{begin, check, end, keys, planet, put_at_seat, ship_e, ship_vel, sit, tap, teleport_ship, with_ship, Ctx, Step};
use avian3d::prelude::{Position, Rotation};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use flight_core::{AxisTuning, FlightModel, PlanetEnv};
use std::sync::{Arc, Mutex};

const MODELS: [FlightModel; 2] = [FlightModel::Classic, FlightModel::Axis];
/// m above the ground: the start of every manoeuvre but the landing.
const START_HEIGHT: f64 = 400.0;
const LAND_HEIGHT: f64 = 100.0;
/// m above the ground: out of the atmosphere and the gravity field (they end at 1200 and 6000 m).
const SPACE_HEIGHT: f64 = 8000.0;

/// The numbers: one row per measure, one column per model.
#[derive(Default)]
struct Table {
    rows: Vec<(&'static str, [f64; 2])>,
}

type Shared = Arc<Mutex<Table>>;

fn put(t: &Shared, model: usize, name: &'static str, value: f64) {
    let mut t = t.lock().unwrap();
    match t.rows.iter_mut().find(|r| r.0 == name) {
        Some(r) => r.1[model] = value,
        None => {
            let mut v = [f64::NAN; 2];
            v[model] = value;
            t.rows.push((name, v));
        }
    }
}

fn pose(w: &mut World) -> (DVec3, DQuat) {
    let e = ship_e(w);
    (w.get::<Position>(e).unwrap().0, w.get::<Rotation>(e).unwrap().0)
}

fn axis_tuning(w: &mut World) -> AxisTuning {
    with_ship(w, |s| s.ctl.axis_tuning.clone())
}

/// Level, at rest, `height` m above the ground over the start point (`c.p["start"]`), all keys
/// released; `roll` turns the ship about its nose.
fn place(w: &mut World, c: &Ctx, height: f64, roll_deg: f64) {
    let pl = planet(w);
    let up = pl.up(c.p["start"]);
    let rot = crate::ship::basis_for_up(up) * DQuat::from_rotation_z(roll_deg.to_radians());
    teleport_ship(w, pl.centre + up * (pl.surface(up) + height), rot);
    w.resource_mut::<Controls>().held.clear();
    w.resource_mut::<Controls>().pad_axes.clear();
    with_ship(w, |s| {
        s.ctl.reset_state();
        s.ctl.hover_assist = true;
    });
}

/// Per tick: speed, felt acceleration (what the hull's forces give, gravity taken out) and the
/// angle between nose and velocity.
#[derive(Default)]
struct Probe {
    v_prev: Option<DVec3>,
    max_g: f64,
    max_slip: f64,
    dist: f64,
}

impl Probe {
    fn tick(&mut self, w: &mut World, c: &Ctx) -> DVec3 {
        let v = ship_vel(w);
        let (pos, rot) = pose(w);
        if let Some(v0) = self.v_prev {
            let g = w.resource::<PlanetRes>().gravity_at(pos);
            self.max_g = self.max_g.max(((v - v0) / c.dt.max(1e-9) - g).length() / 9.81);
            self.dist += v.length() * c.dt;
        }
        if v.length() > 5.0 {
            self.max_slip = self.max_slip.max((rot * DVec3::NEG_Z).angle_between(v).to_degrees());
        }
        self.v_prev = Some(v);
        v
    }
}

/// One manoeuvre for model `m`: `drive` runs every tick from the placed pose and returns true when done.
fn manoeuvre(m: usize, name: &'static str, height: f64, roll_deg: f64, mut drive: impl FnMut(&mut World, &mut Ctx, &mut Probe, &Shared) -> bool + Send + Sync + 'static, table: &Shared) -> Step {
    let table = table.clone();
    let mut probe = Probe::default();
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, &format!("{} {name}", MODELS[m].label()));
            place(w, c, height, roll_deg);
            probe = Probe::default();
        }
        probe.tick(w, c);
        let done = drive(w, c, &mut probe, &table);
        if done {
            w.resource_mut::<Controls>().held.clear();
            w.resource_mut::<Controls>().pad_axes.clear();
            end(w, c, format!("{:.1} s", c.t));
        }
        done
    })
}

/// Switch to model `m` with F7 (through the bindings) and check the ship and the HUD.
fn switch_to(m: usize) -> Vec<Step> {
    vec![
        Box::new(move |w, _| {
            if with_ship(w, |s| s.ctl.model) != MODELS[m] {
                tap(w, KeyCode::F7);
            }
            true
        }),
        Box::new(move |w, c| {
            let model = with_ship(w, |s| s.ctl.model);
            // The readout runs after the step; one tick later it names the model.
            if c.t < 0.05 {
                return false;
            }
            let shown = w.resource::<HudReadout>().flight_model;
            check(c, model == MODELS[m] && shown.starts_with(MODELS[m].label()), format!("F7: model {model:?}, HUD {shown:?}"));
            true
        }),
    ]
}

fn steps_for(m: usize, s: &mut Vec<Step>, t: &Shared) {
    s.extend(switch_to(m));
    // W from rest, then release: acceleration, speed, coasting to rest.
    s.push(manoeuvre(m, "W from rest, then release", START_HEIGHT, 0.0, {
        let (mut t50, mut speeds) = (f64::NAN, Vec::new());
        move |w, c, p, t| {
            let v = ship_vel(w).length();
            if c.t == 0.0 {
                (t50, speeds) = (f64::NAN, Vec::new());
                keys(w, &[KeyCode::KeyW], true);
            }
            if c.t < 15.0 {
                if v >= 50.0 && t50.is_nan() {
                    t50 = c.t;
                }
                speeds.push((c.t, v));
                return false;
            }
            if !c.v.contains_key("released") {
                c.v.insert("released", c.t);
                c.v.insert("dist0", p.dist);
                keys(w, &[KeyCode::KeyW], false);
                let t90 = speeds.iter().find(|(_, s)| *s >= 0.9 * v).map_or(f64::NAN, |x| x.0);
                put(t, m, "W: time to 50 m/s (s)", t50);
                put(t, m, "W: speed after 15 s (m/s)", v);
                put(t, m, "W: time to 90 % of it (s)", t90);
                put(t, m, "W: highest felt g", p.max_g);
                if MODELS[m] == FlightModel::Axis {
                    // The cap at this height: between the atmosphere's and the space one by the density.
                    let cap = with_ship(w, |s| s.ctl.forward_speed_limit);
                    check(c, (v - cap).abs() < 0.03 * cap, format!("axis: W for 15 s reaches the cruise cap at this height, {cap:.1} m/s ({v:.1} m/s)"));
                }
                return false;
            }
            let since = c.t - c.v["released"];
            if v < 1.0 || since > 40.0 {
                put(t, m, "release: time to < 1 m/s (s)", since);
                put(t, m, "release: distance (m)", p.dist - c.v["dist0"]);
                c.v.remove("released");
                return true;
            }
            false
        }
    }, t));
    // The brake from cruise.
    s.push(manoeuvre(m, "W 10 s, then X", START_HEIGHT, 0.0, move |w, c, p, t| {
        let v = ship_vel(w).length();
        if c.t == 0.0 {
            keys(w, &[KeyCode::KeyW], true);
        }
        if c.t < 10.0 {
            return false;
        }
        if !c.v.contains_key("braking") {
            c.v.insert("braking", c.t);
            c.v.insert("dist0", p.dist);
            put(t, m, "X: speed at the brake (m/s)", v);
            keys(w, &[KeyCode::KeyW], false);
            keys(w, &[KeyCode::KeyX], true);
        }
        let since = c.t - c.v["braking"];
        if v < 0.5 || since > 30.0 {
            put(t, m, "X: time to < 0.5 m/s (s)", since);
            put(t, m, "X: distance (m)", p.dist - c.v["dist0"]);
            c.v.remove("braking");
            return true;
        }
        false
    }, t));
    // Sideways.
    s.push(manoeuvre(m, "D 6 s", START_HEIGHT, 0.0, {
        let mut speeds = Vec::new();
        move |w, c, _, t| {
            let (_, rot) = pose(w);
            let side = (rot.inverse() * ship_vel(w)).x;
            if c.t == 0.0 {
                speeds.clear();
                keys(w, &[KeyCode::KeyD], true);
            }
            speeds.push((c.t, side));
            if c.t >= 6.0 {
                put(t, m, "D: side speed after 6 s (m/s)", side);
                put(t, m, "D: time to 90 % of it (s)", speeds.iter().find(|(_, s)| *s >= 0.9 * side).map_or(f64::NAN, |x| x.0));
                return true;
            }
            false
        }
    }, t));
    // Up and down.
    for (name, key, row) in [("Space 5 s", KeyCode::Space, "Space: climb after 5 s (m/s)"), ("Ctrl 5 s", KeyCode::ControlLeft, "Ctrl: sink after 5 s (m/s)")] {
        s.push(manoeuvre(m, name, START_HEIGHT, 0.0, move |w, c, _, t| {
            if c.t == 0.0 {
                keys(w, &[key], true);
            }
            if c.t >= 5.0 {
                let (pos, _) = pose(w);
                let vertical = ship_vel(w).dot(planet(w).up(pos));
                put(t, m, row, if key == KeyCode::Space { vertical } else { -vertical });
                return true;
            }
            false
        }, t));
    }
    // A full right turn at cruise speed, W held.
    s.push(manoeuvre(m, "W 12 s, then full right stick 5 s", START_HEIGHT, 0.0, move |w, c, p, t| {
        let (pos, rot) = pose(w);
        // The heading: the nose on the local horizontal plane (horizon follow pitches it).
        let up = planet(w).up(pos);
        let nose = rot * DVec3::NEG_Z;
        let nose = (nose - up * nose.dot(up)).normalize_or_zero();
        if c.t == 0.0 {
            keys(w, &[KeyCode::KeyW], true);
        }
        if c.t < 12.0 {
            return false;
        }
        if !c.p.contains_key("nose0") {
            c.p.insert("nose0", nose);
            c.v.insert("turn0", c.t);
            c.v.insert("turned", 0.0);
            put(t, m, "turn: speed at the start (m/s)", ship_vel(w).length());
            p.max_slip = 0.0;
            p.max_g = 0.0;
            w.resource_mut::<Controls>().pad_axes.insert(GamepadAxis::RightStickX, 1.0);
        }
        // Summed tick by tick: more than half a turn in 5 s is possible.
        let turned = c.v["turned"] + c.p["nose0"].angle_between(nose).to_degrees();
        c.v.insert("turned", turned);
        c.p.insert("nose0", nose);
        if c.t - c.v["turn0"] >= 5.0 {
            let v = ship_vel(w);
            let e = ship_e(w);
            let rate = (rot.inverse() * w.get::<avian3d::prelude::AngularVelocity>(e).unwrap().0).y;
            put(t, m, "turn: heading change in 5 s (deg)", turned);
            put(t, m, "turn: yaw rate at the end (deg/s)", -rate.to_degrees());
            put(t, m, "turn: largest slip nose/velocity (deg)", p.max_slip);
            put(t, m, "turn: speed at the end (m/s)", v.length());
            put(t, m, "turn: highest felt g", p.max_g);
            if MODELS[m] == FlightModel::Axis {
                let a = axis_tuning(w);
                let top = a.rate.yaw * a.rate_over_speed.points.iter().map(|p| p.y).fold(0.0, f64::max);
                if a.g_safety.cap_turns {
                    // A right yaw pulls the forward velocity to the right.
                    let cap = a.g_safety.limit.right * flight_core::axis::G0 / -(rot.inverse() * v).z;
                    check(c, -rate <= cap.min(top) * 1.05, format!("axis: G-safety caps the yaw rate at {:.1} m/s: {:.3} rad/s (cap {cap:.3})", v.length(), -rate));
                } else {
                    check(c, -rate <= top * 1.02 && -rate > 0.5 * a.rate.yaw, format!("axis: the nose turns at its rate over the speed at {:.1} m/s: {:.3} rad/s (top {top:.3})", v.length(), -rate));
                }
            }
            c.p.remove("nose0");
            return true;
        }
        false
    }, t));
    // Boost from rest; the capacitor is full.
    s.push(manoeuvre(m, "W + Shift 10 s", START_HEIGHT, 0.0, move |w, c, _, t| {
        let v = ship_vel(w).length();
        if c.t == 0.0 {
            c.v.insert("boost_max", 0.0);
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
        }
        let max = c.v["boost_max"].max(v);
        c.v.insert("boost_max", max);
        if c.t >= 3.0 && !c.v.contains_key("boost3") {
            c.v.insert("boost3", v);
            put(t, m, "boost: speed after 3 s (m/s)", v);
        }
        if c.t >= 10.0 {
            put(t, m, "boost: top speed (m/s)", max);
            put(t, m, "boost: speed after 10 s (m/s)", v);
            c.v.remove("boost3");
            return true;
        }
        false
    }, t));
    // Decoupled glide (C): the blend takes `decouple_time`, then 8 s without input.
    s.push(manoeuvre(m, "W 12 s, then C and release", START_HEIGHT, 0.0, move |w, c, _, t| {
        let v = ship_vel(w).length();
        if c.t == 0.0 {
            keys(w, &[KeyCode::KeyW], true);
        }
        if c.t < 12.0 {
            return false;
        }
        if !c.v.contains_key("glide0") {
            c.v.insert("glide0", c.t);
            c.v.insert("glide_v0", v);
            keys(w, &[KeyCode::KeyW], false);
            tap(w, KeyCode::KeyC);
        }
        let since = c.t - c.v["glide0"];
        let blend = with_ship(w, |s| s.ctl.tuning.decouple_time);
        if since >= blend + 8.0 {
            let v0 = c.v["glide_v0"];
            put(t, m, "C: speed kept after the blend + 8 s (%)", 100.0 * v / v0);
            check(c, !with_ship(w, |s| s.ctl.coupled) && v > 0.5 * v0, format!("{}: decoupled glide keeps {:.0} % of {v0:.1} m/s", MODELS[m].label(), 100.0 * v / v0));
            tap(w, KeyCode::KeyC);
            c.v.remove("glide0");
            return true;
        }
        false
    }, t));
    // Rolled on its side, no input.
    s.push(manoeuvre(m, "rolled 90 deg, no input 5 s", START_HEIGHT, 90.0, move |w, c, _, t| {
        let (pos, _) = pose(w);
        let h = planet(w).above_ground(pos);
        if c.t == 0.0 {
            c.v.insert("h0", h);
        }
        if c.t >= 5.0 {
            let lost = c.v["h0"] - h;
            put(t, m, "rolled: height lost in 5 s (m)", lost);
            if MODELS[m] == FlightModel::Axis {
                let side = axis_tuning(w).accel.right.min(axis_tuning(w).accel.left);
                let holds = side >= 9.81 * 1.02;
                check(c, if holds { lost.abs() < 1.0 } else { lost > 1.0 }, format!("axis: rolled with {side} m/s² sideways the ship {} ({lost:.2} m lost)", if holds { "holds" } else { "sinks" }));
            }
            return true;
        }
        false
    }, t));
    // Out of the atmosphere and the field: full thrust from rest.
    s.push(manoeuvre(m, "space: W 20 s", SPACE_HEIGHT, 0.0, {
        let mut speeds = Vec::new();
        move |w, c, _, t| {
            let v = ship_vel(w).length();
            if c.t == 0.0 {
                speeds.clear();
                keys(w, &[KeyCode::KeyW], true);
            }
            speeds.push((c.t, v));
            if c.t >= 20.0 {
                put(t, m, "space W: speed after 20 s (m/s)", v);
                put(t, m, "space W: time to 90 % of it (s)", speeds.iter().find(|(_, s)| *s >= 0.9 * v).map_or(f64::NAN, |x| x.0));
                if MODELS[m] == FlightModel::Axis {
                    let cap = axis_tuning(w).space.cruise_speed;
                    check(c, (v - cap).abs() < 0.02 * cap, format!("axis: in space W reaches the space cruise speed {cap} ({v:.1} m/s)"));
                }
                return true;
            }
            false
        }
    }, t));
    // Decoupled in space: W for 15 s, then X until it stops.
    s.push(manoeuvre(m, "space: C, W 15 s, then X", SPACE_HEIGHT, 0.0, move |w, c, p, t| {
        let v = ship_vel(w).length();
        if c.t == 0.0 {
            tap(w, KeyCode::KeyC);
            keys(w, &[KeyCode::KeyW], true);
        }
        if c.t < 15.0 {
            return false;
        }
        if !c.v.contains_key("space_x") {
            c.v.insert("space_x", c.t);
            c.v.insert("dist0", p.dist);
            put(t, m, "space C: speed after 15 s of W (m/s)", v);
            if MODELS[m] == FlightModel::Axis {
                let cap = axis_tuning(w).space.cruise_speed;
                check(c, v <= cap * 1.02, format!("axis: decoupled W stops at the space cruise cap {cap} ({v:.1} m/s)"));
            }
            keys(w, &[KeyCode::KeyW], false);
            keys(w, &[KeyCode::KeyX], true);
        }
        let since = c.t - c.v["space_x"];
        if v < 0.5 || since > 60.0 {
            put(t, m, "space C: X to < 0.5 m/s (s)", since);
            put(t, m, "space C: X distance (m)", p.dist - c.v["dist0"]);
            check(c, v < 0.5, format!("{}: X stops the decoupled ship in space ({since:.1} s)", MODELS[m].label()));
            tap(w, KeyCode::KeyC);
            c.v.remove("space_x");
            return true;
        }
        false
    }, t));
    // Landing with Ctrl held from 100 m until it rests.
    s.push(manoeuvre(m, "land from 100 m with Ctrl", LAND_HEIGHT, 0.0, {
        let mut sinks: Vec<(f64, f64)> = Vec::new();
        move |w, c, _, t| {
        let v = ship_vel(w);
        let (pos, _) = pose(w);
        let up = planet(w).up(pos);
        if c.t == 0.0 {
            c.p.remove("touch");
            sinks.clear();
            keys(w, &[KeyCode::ControlLeft], true);
            // Landing mode (K, the axis model only), through the bindings.
            tap(w, KeyCode::KeyK);
        }
        if (c.t - c.dt).abs() < 1e-9 && MODELS[m] == FlightModel::Axis {
            let shown = w.resource::<HudReadout>().flight_model;
            check(c, with_ship(w, |s| s.ctl.landing_mode) && shown == "AXIS LANDING", format!("K: landing mode on, HUD {shown:?}"));
        }
        // `grounded` comes a step after the contact, and the solver has cut the approach by then:
        // the touchdown speed is the largest sink of the last 0.25 s before it.
        sinks.push((c.t, -v.dot(up)));
        let grounded = with_ship(w, |s| s.grounded);
        if grounded && !c.p.contains_key("touch") {
            c.p.insert("touch", pos);
            c.v.insert("touch_t", c.t);
            let sink = sinks.iter().filter(|(ts, _)| *ts >= c.t - 0.25).map(|x| x.1).fold(f64::NAN, f64::max);
            put(t, m, "land: time to touchdown (s)", c.t);
            put(t, m, "land: sink at touchdown (m/s)", sink);
        }
        let rested = c.p.contains_key("touch") && c.t - c.v["touch_t"] > 1.0 && v.length() < 0.05;
        if rested || c.t >= 90.0 {
            let slid = c.p.get("touch").map_or(f64::NAN, |t0| {
                let d = pos - *t0;
                (d - up * d.dot(up)).length()
            });
            tap(w, KeyCode::KeyK);
            put(t, m, "land: slide after touchdown (m)", slid);
            put(t, m, "land: time to rest (s)", c.t);
            check(c, rested && slid < 0.5, format!("{}: lands and rests without sliding ({slid:.3} m, {:.1} s)", MODELS[m].label(), c.t));
            if MODELS[m] == FlightModel::Axis {
                let a = axis_tuning(w);
                let cap = a.precision.speed * a.precision.landing_share;
                let sink = t.lock().unwrap().rows.iter().find(|r| r.0 == "land: sink at touchdown (m/s)").map_or(f64::NAN, |r| r.1[m]);
                check(c, sink <= cap + 0.5, format!("axis: touchdown at {sink:.2} m/s (landing cap {cap})"));
            }
            return true;
        }
        false
    }}, t));
}

pub fn flight_models_steps(s: &mut Vec<Step>) {
    let table: Shared = Arc::default();
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(Box::new(|w, c| {
        let (pos, _) = pose(w);
        c.p.insert("start", pos);
        let model = with_ship(w, |s| s.ctl.model);
        check(c, model == FlightModel::Classic && w.resource::<HudReadout>().flight_model == "CLASSIC", format!("seated: classic model by default ({model:?}, HUD {:?})", w.resource::<HudReadout>().flight_model));
        true
    }));
    for m in 0..MODELS.len() {
        steps_for(m, s, &table);
    }
    s.extend(switch_to(0));
    s.push(Box::new(move |_, c| {
        let t = table.lock().unwrap();
        let mut lines = vec![format!("{:<44}{:>10}{:>10}", "FLIGHT MODELS (spike 13)", MODELS[0].label(), MODELS[1].label())];
        for (name, v) in &t.rows {
            lines.push(format!("{name:<44}{:>10.2}{:>10.2}", v[0], v[1]));
        }
        for l in lines {
            println!("{l}");
            c.report.push(l);
        }
        let bad: Vec<_> = t.rows.iter().filter(|r| !r.1.iter().all(|x| x.is_finite())).map(|r| r.0).collect();
        check(c, bad.is_empty(), format!("every manoeuvre measured with both models (missing: {bad:?})"));
        true
    }));
}
