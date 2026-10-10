//! Scenario `sc-linear` (round 5, #195): the SC model's linear law, driven through `Controls` from
//! 400 m: D then W with boost (the peak stays under the boost cap), W+D then X (the heading holds
//! while braking), H off (the ship falls with g), H on (it holds), B then W (NAV flies above the
//! SCM cap, and bleeds back to it), PageDown five times (half the cap). Each line is a check with
//! its numbers. Comstab off: brake in space, turn on the spot, launch along the new nose.
use crate::controls::Controls;
use crate::scenario::{altitude, begin, check, end, keys, planet, put_at_seat, ship_e, ship_vel, sit, tap, teleport_ship, with_ship, wait, Ctx, Step};
use crate::ship::{basis_for_up, FlightModel};
use avian3d::prelude::{AngularVelocity, Position, Rotation};
use bevy::math::DVec3;
use bevy::prelude::*;
use flight_core::PlanetEnv;

/// m above the ground the manoeuvres start from.
const LIFT_HEIGHT: f64 = 400.0;

/// Teleports the ship to `LIFT_HEIGHT` above the spot it stands on, level, at rest, the switches
/// as at the start (test setup).
fn lift(w: &mut World) {
    let e = ship_e(w);
    let p = w.get::<Position>(e).unwrap().0;
    let pl = planet(w);
    let up = pl.up(p);
    teleport_ship(w, pl.centre + up * (pl.surface(up) + LIFT_HEIGHT), basis_for_up(up));
    w.resource_mut::<Controls>().held.clear();
    w.resource_mut::<Controls>().pad_axes.clear();
    with_ship(w, |s| s.sc.reset_state());
}

/// Holds `ks` for `secs` and runs `tick` every tick (the keys go up at the end).
fn timed(name: &'static str, ks: Vec<KeyCode>, secs: f64, mut tick: impl FnMut(&mut World, &mut Ctx) + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            keys(w, &ks, true);
        }
        tick(w, c);
        if c.t >= secs {
            keys(w, &ks, false);
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        false
    })
}

/// The up direction of the planet at the ship.
fn ship_up(w: &mut World) -> DVec3 {
    let e = ship_e(w);
    let p = w.get::<Position>(e).unwrap().0;
    planet(w).up(p)
}

/// The SC status' forward cap in force (m/s).
fn cap_in_force(w: &mut World) -> f64 {
    with_ship(w, |s| s.sc.status.cap)
}

fn model_is_sc(w: &mut World) -> bool {
    with_ship(w, |s| s.model) == FlightModel::Sc
}

pub fn sc_linear_steps(s: &mut Vec<Step>) {
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
        check(c, model_is_sc(w), "F7: the SC model is on".into());
        lift(w);
        true
    }));
    s.push(wait(1.0));

    // D then W with boost: sideways to the cap, then forward under the boost cap.
    s.push(timed("D to the sideways cap, 8 s", vec![KeyCode::KeyD], 8.0, |w, c| {
        let speed = ship_vel(w).length();
        c.v.insert("sideways", speed);
    }));
    s.push(Box::new(|w, c| {
        // At 400 m the air lowers the caps: the cap in force is the status' forward cap (no boost).
        let (cap, boost_cap) = with_ship(w, |s| (s.sc.status.cap, s.sc.tuning.linear.scm.boost_forward));
        let speed = c.v["sideways"];
        // In air the sideways drag (sc_air.json) eats the weak side thrusters before the cap: the
        // strafe settles well below it (#199), so the check asks for a real strafe, not the cap.
        check(c, speed > 30.0 && speed <= cap + 0.5, format!("D: {speed:.1} m/s sideways after 8 s (cap in force {cap:.1}, air-limited)"));
        c.v.insert("boost_cap", boost_cap);
        true
    }));
    s.push(timed("W with boost, 3 s", vec![KeyCode::KeyW, KeyCode::ShiftLeft], 3.0, |w, c| {
        let peak = c.v.get("peak").copied().unwrap_or(0.0).max(ship_vel(w).length());
        c.v.insert("peak", peak);
    }));
    s.push(Box::new(|_, c| {
        let (peak, cap) = (c.v["peak"], c.v["boost_cap"]);
        check(c, peak <= cap + 0.5, format!("D then W with boost: peak {peak:.1} m/s, boost cap {cap}"));
        true
    }));
    s.push(wait(1.0));

    // W+D to the cruise, then X: the heading holds while the ship stops.
    s.push(timed("W+D to the cruise, 6 s", vec![KeyCode::KeyW, KeyCode::KeyD], 6.0, |_, _| {}));
    let mut h0: Option<DVec3> = None;
    let mut max_turn = 0.0_f64;
    let mut stopped: Option<f64> = None;
    s.push(timed("X: brake, 8 s", vec![KeyCode::KeyX], 8.0, move |w, c| {
        let v = ship_vel(w);
        let speed = v.length();
        let h = *h0.get_or_insert(v.normalize_or_zero());
        if speed > 1.0 {
            max_turn = max_turn.max(v.normalize().angle_between(h).to_degrees());
        }
        if speed < 0.05 && stopped.is_none() {
            stopped = Some(c.t);
        }
        c.v.insert("turn", max_turn);
        c.v.insert("stop", stopped.unwrap_or(-1.0));
    }));
    s.push(Box::new(|w, c| {
        let (turn, stop) = (c.v["turn"], c.v["stop"]);
        let _ = w;
        // In air the brake's thrust builds up at its jerk (#198) while drag still pulls: about a
        // degree of turn (in space the core test holds it under 0.5 deg).
        check(c, turn < 2.0, format!("X: the heading turned {turn:.3} deg until 1 m/s"));
        check(c, stop >= 0.0, format!("X: stopped after {stop:.2} s"));
        true
    }));
    s.push(wait(1.0));

    // H off: the ship falls with g. The fall is measured over the ticks the step ran (c.t), the
    // tap is taken on the step's first tick.
    let mut fall0: Option<(f64, f64)> = None;
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "H off: free fall, 1 s");
            let e = ship_e(w);
            let p = w.get::<Position>(e).unwrap().0;
            let g = w.resource::<crate::env::PlanetRes>().gravity_at(p).length();
            fall0 = Some((altitude(w), g));
            tap(w, KeyCode::KeyH);
            return false;
        }
        if c.t < 1.0 {
            return false;
        }
        let (alt0, g) = fall0.unwrap_or((0.0, 0.0));
        let fallen = alt0 - altitude(w);
        let expect = 0.5 * g * c.t * c.t;
        let grav_comp = with_ship(w, |s| s.sc.status.grav_comp);
        end(w, c, format!("{:.2} s, fell {fallen:.2} m", c.t));
        check(c, !grav_comp, "H off: compensation off".into());
        check(c, (fallen - expect).abs() < 0.05 * expect, format!("H off: fell {fallen:.2} m in {:.2} s, g t^2/2 {expect:.2} m (g {g:.2})", c.t));
        true
    }));

    // H on: the flight computer holds the ship again.
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyH);
        true
    }));
    s.push(timed("H on: hold, 3 s", vec![], 3.0, |_, _| {}));
    s.push(Box::new(|w, c| {
        let up = ship_up(w);
        let vertical = ship_vel(w).dot(up);
        let grav_comp = with_ship(w, |s| s.sc.status.grav_comp);
        check(c, grav_comp, "H on: compensation on".into());
        check(c, vertical.abs() < 1.0, format!("H on: vertical speed {vertical:.2} m/s after 3 s"));
        true
    }));

    // B then W: NAV flies above the SCM cap; back to SCM it bleeds to the cap.
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyB);
        true
    }));
    s.push(timed("NAV: W, 15 s", vec![KeyCode::KeyW], 15.0, |w, c| {
        let peak = c.v.get("nav_peak").copied().unwrap_or(0.0).max(ship_vel(w).length());
        c.v.insert("nav_peak", peak);
    }));
    s.push(Box::new(|w, c| {
        let (peak, scm) = (c.v["nav_peak"], with_ship(w, |s| s.sc.tuning.linear.scm.cruise));
        let master = with_ship(w, |s| s.sc.status.master);
        check(c, master == flight_core::sc::Master::Nav, "B: NAV master mode".into());
        check(c, peak > scm + 20.0, format!("NAV: peak {peak:.1} m/s, SCM cruise {scm}"));
        tap(w, KeyCode::KeyB);
        true
    }));
    // From NAV speed the bleed takes a while at the thrusters' ~80 m/s² (SC-derived values).
    s.push(timed("back to SCM: W, 20 s", vec![KeyCode::KeyW], 20.0, |_, _| {}));
    s.push(Box::new(|w, c| {
        let speed = ship_vel(w).length();
        let cap = cap_in_force(w);
        check(c, (speed - cap).abs() < 2.0, format!("SCM after NAV: speed {speed:.1} m/s, cap {cap:.1} m/s"));
        true
    }));

    // PageDown five times: the limiter at half, the ship holds half the cap.
    for _ in 0..5 {
        s.push(Box::new(|w, _| {
            tap(w, KeyCode::PageDown);
            true
        }));
        s.push(wait(0.1));
    }
    s.push(timed("limiter 0.5: W, 10 s", vec![KeyCode::KeyW], 10.0, |_, _| {}));
    s.push(Box::new(|w, c| {
        let limiter = with_ship(w, |s| s.sc.status.limiter);
        let speed = ship_vel(w).length();
        let cap = cap_in_force(w);
        check(c, (limiter - 0.5).abs() < 1e-6, format!("PageDown x5: limiter {limiter:.2}"));
        check(c, (speed - cap).abs() < 2.0, format!("limiter 0.5: speed {speed:.1} m/s, cap {cap:.1} m/s"));
        true
    }));

    // Playtest regression: coupled, comstab off, brake to rest, turn, then W. In vacuum the
    // old heading cannot be confused with wind, gravity or a curved planetary horizon.
    s.push(Box::new(|w, _| {
        lift(w);
        let e = ship_e(w);
        let p = w.get::<Position>(e).unwrap().0;
        let pl = planet(w);
        let up = pl.up(p);
        teleport_ship(w, pl.centre + up * (pl.radius + 8000.0), basis_for_up(up));
        tap(w, KeyCode::KeyU);
        true
    }));
    s.push(timed("comstab off: W in space, 5 s", vec![KeyCode::KeyW], 5.0, |_, _| {}));
    s.push(Box::new(|w, c| {
        let (coupled, comstab) = with_ship(w, |s| (s.sc.status.coupled, s.sc.status.comstab));
        check(c, coupled && !comstab, "rest-heading regression: coupled, comstab off".into());
        check(c, ship_vel(w).length() > 20.0, "rest-heading regression: moving before braking".into());
        true
    }));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "comstab off: X to exact rest");
            keys(w, &[KeyCode::KeyX], true);
            return false;
        }
        let speed = ship_vel(w).length();
        if speed == 0.0 || c.t >= 10.0 {
            keys(w, &[KeyCode::KeyX], false);
            check(c, speed == 0.0, format!("comstab off: X reaches exact rest ({speed:.12} m/s after {:.2} s)", c.t));
            end(w, c, format!("{speed:.12} m/s"));
            return true;
        }
        false
    }));
    let mut old_nose = DVec3::ZERO;
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "comstab off: turn on the spot, 2 s");
            let e = ship_e(w);
            old_nose = w.get::<Rotation>(e).unwrap().0 * DVec3::NEG_Z;
            w.resource_mut::<Controls>().pad_axes.insert(GamepadAxis::RightStickX, 1.0);
        }
        if c.t >= 2.0 {
            w.resource_mut::<Controls>().pad_axes.remove(&GamepadAxis::RightStickX);
            let e = ship_e(w);
            let nose = w.get::<Rotation>(e).unwrap().0 * DVec3::NEG_Z;
            let turn = nose.angle_between(old_nose).to_degrees();
            check(c, turn > 30.0, format!("comstab off: turned {turn:.2} deg on the spot"));
            end(w, c, format!("{turn:.2} deg"));
            return true;
        }
        false
    }));
    s.push(Box::new(|w, c| {
        let e = ship_e(w);
        let spin = w.get::<AngularVelocity>(e).unwrap().0.length();
        if spin < 0.001 || c.t >= 3.0 {
            check(c, spin < 0.001, format!("comstab off: rotation settled ({spin:.6} rad/s)"));
            check(c, ship_vel(w).length() < 0.005, "comstab off: still at rest after turning".into());
            return true;
        }
        false
    }));
    s.push(timed("comstab off: W along the new nose, 1 s", vec![KeyCode::KeyW], 1.0, |_, _| {}));
    s.push(Box::new(|w, c| {
        let e = ship_e(w);
        let nose = w.get::<Rotation>(e).unwrap().0 * DVec3::NEG_Z;
        let velocity = ship_vel(w);
        let angle = if velocity.length() > 0.0 { velocity.angle_between(nose).to_degrees() } else { 180.0 };
        check(c, velocity.dot(nose) > 20.0 && angle < 0.5,
            format!("comstab off: launch at {:.2} m/s, {angle:.4} deg from the new nose", velocity.length()));
        true
    }));
}
