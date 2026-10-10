//! Scenario `sc-air` (#199): the SC model's air through `Controls`. Sit, F7 to the SC model, then
//! 50 m over the ground at rest: hover 5 s with wind compensation off (Y) and the ship drifts
//! downwind; on again (Y), it holds. Down to 20 m, fly forward at speed: the turbulence shows. Climb
//! to 1000 m with Space: no turbulence up there. Each result is a check line with its numbers.
use crate::controls::Controls;
use crate::scenario::{altitude, begin, check, end, keys, hold_until, planet, put_at_seat, ship_e, ship_vel, sit, tap, teleport_ship, with_ship, Step};
use crate::ship::{basis_for_up, FlightModel};
use avian3d::prelude::Position;
use bevy::math::DVec3;
use bevy::prelude::*;

fn ship_pos(w: &mut World) -> DVec3 {
    let e = ship_e(w);
    w.get::<Position>(e).unwrap().0
}

/// Test setup: level and at rest `height` m above the ground under the ship, no key held.
fn place(w: &mut World, height: f64) {
    let p = ship_pos(w);
    let pl = planet(w);
    let up = pl.up(p);
    teleport_ship(w, pl.centre + up * (pl.surface(up) + height), basis_for_up(up));
    w.resource_mut::<Controls>().held.clear();
    with_ship(w, |s| s.sc.reset_state());
}

/// Holds `ks` for up to `limit` s, or until `until`, and tracks the largest turbulence and speed in
/// `c.v["turb_max"]` and `c.v["speed_max"]`.
fn track(name: &'static str, ks: &'static [KeyCode], limit: f64, until: fn(&mut World) -> bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            c.v.insert("turb_max", 0.0);
            c.v.insert("speed_max", 0.0);
            keys(w, ks, true);
        }
        let turb = with_ship(w, |s| s.sc.status.turbulence);
        let speed = ship_vel(w).length();
        let turb_max = c.v["turb_max"].max(turb);
        let speed_max = c.v["speed_max"].max(speed);
        c.v.insert("turb_max", turb_max);
        c.v.insert("speed_max", speed_max);
        if until(w) || c.t >= limit {
            keys(w, ks, false);
            let alt = altitude(w);
            end(w, c, format!("{:.1} s, altitude {alt:.0} m, top speed {speed_max:.0} m/s, turbulence max {turb_max:.2}", c.t));
            return true;
        }
        false
    })
}

fn tap_y() -> Step {
    Box::new(|w, _| {
        tap(w, KeyCode::KeyY);
        true
    })
}

pub fn sc_air_steps(s: &mut Vec<Step>) {
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
        let m = with_ship(w, |s| s.model);
        check(c, m == FlightModel::Sc, format!("F7: model {m:?}, want Sc"));
        true
    }));
    s.push(Box::new(|w, _| {
        place(w, 50.0);
        true
    }));
    s.push(hold_until("settle 2 s (wind compensation on)", &[], 2.0, |_| false));

    // Wind compensation off: the wind pushes the ship downwind.
    s.push(Box::new(|w, c| {
        c.p.insert("h0", ship_pos(w));
        tap(w, KeyCode::KeyY);
        true
    }));
    s.push(hold_until("hover 5 s, wind compensation off", &[], 5.0, |_| false));
    s.push(Box::new(|w, c| {
        let drift = (ship_pos(w) - c.p["h0"]).length();
        let (on, wind) = with_ship(w, |s| (s.sc.status.wind_comp, s.sc.status.wind.length()));
        check(c, !on && drift > 5.0, format!("hover, wind compensation off: drift {drift:.2} m in 5 s (wind {wind:.1} m/s)"));
        true
    }));

    // Wind compensation on again: the flight computer holds against the wind.
    s.push(tap_y());
    s.push(hold_until("settle 4 s (wind compensation on)", &[], 4.0, |_| false));
    s.push(Box::new(|w, c| {
        c.p.insert("h1", ship_pos(w));
        true
    }));
    s.push(hold_until("hover 5 s, wind compensation on", &[], 5.0, |_| false));
    s.push(Box::new(|w, c| {
        let drift = (ship_pos(w) - c.p["h1"]).length();
        let on = with_ship(w, |s| s.sc.status.wind_comp);
        check(c, on && drift < 1.0, format!("hover, wind compensation on: drift {drift:.2} m in 5 s"));
        true
    }));

    // Low and fast: the turbulence near the ground.
    s.push(Box::new(|w, _| {
        place(w, 20.0);
        true
    }));
    s.push(track("fly low 8 s at speed (W)", &[KeyCode::KeyW], 8.0, |_| false));
    s.push(Box::new(|_, c| {
        let (turb, speed) = (c.v["turb_max"], c.v["speed_max"]);
        check(c, turb > 0.2, format!("low at {speed:.0} m/s: turbulence up to {turb:.2} (20 m)"));
        true
    }));

    // Up to 1000 m: no turbulence there.
    s.push(track("climb to 1000 m (Space)", &[KeyCode::Space], 90.0, |w| altitude(w) >= 1000.0));
    s.push(Box::new(|w, c| {
        let turb = with_ship(w, |s| s.sc.status.turbulence);
        let alt = altitude(w);
        check(c, alt >= 1000.0 && turb < 0.05, format!("1000 m: turbulence {turb:.3}, altitude {alt:.0} m"));
        true
    }));
}
