//! Scenario `sc-lift` (round 5, renamed with #206): the SC model from the seat: the ship lifts off,
//! holds its height, flies forward and shows the SC mode word.
use crate::hud::HudReadout;
use crate::scenario::{check, hold_until, planet, put_at_seat, ship_e, ship_vel, sit, Step};
use avian3d::prelude::{Position, Rotation};
use bevy::math::DVec3;
use bevy::prelude::*;

/// m above the planet's centre-relative reference sphere (the terrain does not matter here).
fn height(w: &mut World) -> f64 {
    let e = ship_e(w);
    let p = w.get::<Position>(e).unwrap().0;
    let pl = planet(w);
    (p - pl.centre).length() - pl.radius
}

pub fn sc_lift_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(Box::new(|w, c| {
        c.v.insert("h0", height(w));
        true
    }));
    s.push(hold_until("lift off 2 s", &[KeyCode::Space], 2.0, |_| false));
    s.push(Box::new(|w, c| {
        let up = height(w) - c.v["h0"];
        check(c, up > 5.0, format!("lift off: {up:.1} m up after 2 s of Space"));
        true
    }));
    // Let the climb settle, then measure a hover.
    s.push(hold_until("settle 2 s", &[], 2.0, |_| false));
    s.push(Box::new(|w, c| {
        c.v.insert("h1", height(w));
        true
    }));
    s.push(hold_until("hover 3 s", &[], 3.0, |_| false));
    s.push(Box::new(|w, c| {
        let drift = height(w) - c.v["h1"];
        check(c, drift.abs() < 1.0, format!("hover: {drift:+.2} m in 3 s with gravity compensation"));
        let text = w.resource::<HudReadout>().texts[0].clone();
        check(c, text.starts_with("SHIP SC"), format!("HUD mode: {text:?}"));
        true
    }));
    s.push(hold_until("forward 3 s", &[KeyCode::KeyW], 3.0, |_| false));
    s.push(Box::new(|w, c| {
        let e = ship_e(w);
        let r = w.get::<Rotation>(e).unwrap().0;
        let forward = (r.inverse() * ship_vel(w)).dot(DVec3::NEG_Z);
        check(c, forward > 20.0, format!("forward: {forward:.1} m/s after 3 s of W"));
        true
    }));
}
