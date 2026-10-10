//! Scenario `sc-turn` (round 5, lane `sc-angular`): the SC model's rotation, driven through the pad
//! stick and Q. Sit, F7, 400 m above the ground; a full pitch for 2 s (the overshoot and the
//! settling), a full reversal (a constant deceleration until the spin crosses zero), roll and its
//! release, then a turn at cruise with the G-safe turn cap (F8) on and off. Each check prints its
//! numbers.
use crate::controls::Controls;
use crate::hud::HudReadout;
use crate::scenario::{begin, check, end, hold_until, keys, planet, put_at_seat, ship_e, ship_vel, sit, tap, teleport_ship, with_ship, Ctx, Step};
use avian3d::prelude::{AngularVelocity, Position, Rotation};
use bevy::math::DVec3;
use bevy::prelude::*;
use std::sync::{Arc, Mutex};

/// m above the ground: where the turns start.
const START_HEIGHT: f64 = 400.0;
/// The fixed step of the simulation (s).
const DT: f64 = 1.0 / 60.0;

/// One tick of the rotation under test: the ship's spin (ship space, rad/s) and the flight state.
#[derive(Clone, Copy, Default)]
struct Sample {
    spin: DVec3,
    speed: f64,
    rate_capped: bool,
}

type Log = Arc<Mutex<Vec<Sample>>>;

fn new_log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

/// m above the planet's reference sphere (the terrain does not matter here).
fn height(w: &mut World) -> f64 {
    let e = ship_e(w);
    let p = w.get::<Position>(e).unwrap().0;
    let pl = planet(w);
    (p - pl.centre).length() - pl.radius
}

fn sample(w: &mut World) -> Sample {
    let e = ship_e(w);
    let (rot, ang) = (w.get::<Rotation>(e).unwrap().0, w.get::<AngularVelocity>(e).unwrap().0);
    let capped = with_ship(w, |s| s.sc.status.rate_capped);
    Sample { spin: rot.inverse() * ang, speed: ship_vel(w).length(), rate_capped: capped }
}

/// Runs `secs` with `ks` held, the pad stick at `pitch` and `roll` (Q or nothing), logging every
/// tick into `log`.
fn fly_for(name: &'static str, secs: f64, ks: &'static [KeyCode], pitch: f32, log: Log) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            keys(w, ks, true);
            w.resource_mut::<Controls>().pad_axes.insert(GamepadAxis::RightStickY, pitch);
        }
        let s = sample(w);
        log.lock().unwrap().push(s);
        if c.t >= secs {
            keys(w, ks, false);
            w.resource_mut::<Controls>().pad_axes.remove(&GamepadAxis::RightStickY);
            let (alt, v) = (height(w), ship_vel(w).length());
            end(w, c, format!("{:.1} s, height {alt:.1} m, speed {v:.1} m/s", c.t));
            return true;
        }
        false
    })
}

/// Checks on the samples of one phase (taken out of the log), with the ticks' numbers printed.
fn analyse(name: &'static str, log: Log, f: impl FnOnce(&mut Ctx, &[Sample]) + Send + Sync + 'static) -> Step {
    let mut f = Some(f);
    Box::new(move |_, c| {
        let samples = std::mem::take(&mut *log.lock().unwrap());
        println!("ANALYSE {name}: {} ticks", samples.len());
        if let Some(f) = f.take() {
            f(c, &samples);
        }
        true
    })
}

/// Per tick decelerations (rad/s²) of one axis over the samples, while the spin keeps its sign.
fn decelerations(samples: &[Sample], axis: impl Fn(DVec3) -> f64) -> Vec<f64> {
    let mut out = Vec::new();
    for pair in samples.windows(2) {
        let (a, b) = (axis(pair[0].spin), axis(pair[1].spin));
        if b <= 0.0 {
            break;
        }
        out.push((b - a) / DT);
    }
    out
}

/// Sit at `height` m over the ground under the ship, level, all keys released.
fn place_at_height(w: &mut World, height: f64) {
    let e = ship_e(w);
    let pl = planet(w);
    let p = w.get::<Position>(e).unwrap().0;
    let up = pl.up(p);
    let rot = crate::ship::basis_for_up(up);
    teleport_ship(w, pl.centre + up * (pl.surface(up) + height), rot);
    w.resource_mut::<Controls>().held.clear();
    w.resource_mut::<Controls>().pad_axes.clear();
    with_ship(w, |s| s.sc.reset_state());
}

pub fn sc_turn_steps(s: &mut Vec<Step>) {
    let (pitch, roll) = (new_log(), new_log());
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F7);
        true
    }));
    s.push(Box::new(|w, c| {
        let (model, text) = (with_ship(w, |s| s.model), w.resource::<HudReadout>().texts[0].clone());
        check(c, model == crate::ship::FlightModel::Sc, format!("F7: model {model:?}, HUD {text:?}"));
        true
    }));
    s.push(Box::new(|w, _| {
        place_at_height(w, START_HEIGHT);
        true
    }));
    s.push(hold_until("hover 3 s at 400 m", &[], 3.0, |_| false));
    s.push(Box::new(|w, c| {
        c.v.insert("h0", height(w));
        true
    }));
    s.push(hold_until("hold 1 s", &[], 1.0, |_| false));
    s.push(Box::new(|w, c| {
        let drift = height(w) - c.v["h0"];
        check(c, drift.abs() < 1.0, format!("hover at 400 m: {drift:+.2} m in 1 s"));
        true
    }));

    // The target of a full pitch from rest: the stick's rate times the corner at speed 0.
    s.push(Box::new(|w, c| {
        let t = with_ship(w, |s| s.sc.tuning.angular.clone());
        c.v.insert("target", t.rate.pitch * t.rate_over_speed.eval(0.0));
        c.v.insert("box", with_ship(w, |s| s.sc.torque_box().pitch));
        c.v.insert("share", t.reversal_share);
        c.v.insert("roll_box", with_ship(w, |s| s.sc.torque_box().roll));
        c.v.insert("roll_share", t.roll_release_share);
        true
    }));

    // Full pitch 2 s: the overshoot and the settling.
    s.push(fly_for("full pitch 2 s", 2.0, &[], 1.0, pitch.clone()));
    s.push(analyse("pitch", pitch.clone(), |c, samples| {
        let target = c.v["target"];
        let peak = samples.iter().map(|s| s.spin.x).fold(f64::MIN, f64::max);
        let over = (peak / target - 1.0) * 100.0;
        let at2 = samples[(2.0 / DT) as usize].spin.x;
        check(c, (5.0..=20.0).contains(&over), format!("pitch overshoot {over:+.1} % (target {target:.3} rad/s, peak {peak:.3})"));
        check(c, (at2 / target - 1.0).abs() < 0.02, format!("pitch 2 s after the stick: {:+.2} % of the target", (at2 / target - 1.0) * 100.0));
    }));

    // Full opposite: a constant deceleration until the spin crosses zero.
    s.push(fly_for("full reverse 1.5 s", 1.5, &[], -1.0, pitch.clone()));
    s.push(analyse("reversal", pitch.clone(), |c, samples| {
        let decel = decelerations(samples, |v| v.x);
        let mean = decel.iter().sum::<f64>() / decel.len().max(1) as f64;
        let spread = decel.iter().map(|a| (a - mean).abs() / mean.abs()).fold(0.0, f64::max);
        let want = -c.v["share"] * c.v["box"];
        check(c, decel.len() > 10 && (mean / want - 1.0).abs() < 0.05, format!("reversal: {} ticks, decel {mean:.3} rad/s^2 (want {want:.3}), spread {:.2} %", decel.len(), spread * 100.0));
    }));
    s.push(hold_until("settle 1 s", &[], 1.0, |_| false));

    // Roll: full for 1 s, then released.
    s.push(fly_for("roll 1 s", 1.0, &[KeyCode::KeyQ], 0.0, roll.clone()));
    s.push(analyse("roll", roll.clone(), |c, samples| {
        let rolled = samples.last().map_or(0.0, |s| s.spin.z);
        check(c, rolled > 1.0, format!("roll at full: {rolled:.3} rad/s"));
    }));
    s.push(fly_for("roll release 1.5 s", 1.5, &[], 0.0, roll.clone()));
    s.push(analyse("roll release", roll.clone(), |c, samples| {
        let decel = decelerations(samples, |v| v.z);
        let mean = decel.iter().sum::<f64>() / decel.len().max(1) as f64;
        let stop = samples.iter().position(|s| s.spin.z.abs() < 1e-9);
        let lowest = samples.iter().map(|s| s.spin.z).fold(f64::MAX, f64::min);
        let want = -c.v["roll_share"] * c.v["roll_box"];
        check(c, decel.len() > 5 && (mean / want - 1.0).abs() < 0.05, format!("roll release: {} ticks, decel {mean:.3} rad/s^2 (want {want:.3})", decel.len()));
        check(c, stop.is_some() && lowest > -0.01, format!("roll stops at tick {stop:?}, lowest spin {lowest:+.4} rad/s"));
    }));

    // A turn at cruise: W held for speed, then a full pitch with the G-safe turn cap on and off (F8).
    s.push(hold_until("speed up with W", &[KeyCode::KeyW], 15.0, |w| ship_vel(w).length() > 140.0));
    let (on, off) = (new_log(), new_log());
    s.push(fly_for("turn at cruise, G-safe on", 3.0, &[KeyCode::KeyW], 1.0, on.clone()));
    s.push(analyse("turn at cruise, G-safe on", on.clone(), |c, samples| {
        let capped = samples.iter().any(|s| s.rate_capped);
        let peak = samples.iter().map(|s| s.spin.x.abs()).fold(0.0, f64::max);
        let speed = samples.last().map_or(0.0, |s| s.speed);
        check(c, capped, format!("G-safe on at {speed:.0} m/s: rate capped {capped}, peak pitch {peak:.3} rad/s"));
    }));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F8);
        true
    }));
    s.push(Box::new(|w, c| {
        let g = with_ship(w, |s| s.sc.modes.g_safe);
        check(c, !g, format!("F8: G-safe {g} (off)"));
        let text = w.resource::<HudReadout>().texts[0].clone();
        println!("HUD {text:?}");
        true
    }));
    s.push(hold_until("settle 1 s at cruise", &[KeyCode::KeyW], 1.0, |_| false));
    s.push(fly_for("turn at cruise, G-safe off", 3.0, &[KeyCode::KeyW], 1.0, off.clone()));
    s.push(analyse("turn at cruise, G-safe off", off.clone(), |c, samples| {
        let capped = samples.iter().any(|s| s.rate_capped);
        let peak = samples.iter().map(|s| s.spin.x.abs()).fold(0.0, f64::max);
        let speed = samples.last().map_or(0.0, |s| s.speed);
        check(c, !capped, format!("G-safe off at {speed:.0} m/s: rate capped {capped}, peak pitch {peak:.3} rad/s"));
    }));
}
