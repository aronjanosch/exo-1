//! The flight HUD's readout (SC model): strafe, forward, limiter and level horizon.
use crate::hud::HudReadout;
use crate::scenario::{check, hold_until, keys, planet, put_at_seat, ship_e, sit, tap, teleport_ship, Step};
use avian3d::prelude::Position;
use bevy::math::DVec3;
use bevy::prelude::*;

/// The flight part of the readout (set from the seat or a cabin).
fn flight(w: &World) -> crate::hud::FlightHud {
    w.resource::<HudReadout>().flight.clone().expect("flight HUD readout while seated")
}

pub fn sc_flight_hud_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(hold_until("settle 0.2 s", &[], 0.2, |_| false));
    s.push(Box::new(|w, c| {
        let f = flight(w);
        check(c, f.speed_tape.limiter.is_none(), "limiter mark off at full speed".to_string());
        true
    }));

    s.push(hold_until("lift 2 s", &[KeyCode::Space], 2.0, |_| false));
    s.push(hold_until("settle 2 s", &[], 2.0, |_| false));
    s.push(Box::new(|w, c| {
        let f = flight(w);
        check(c, f.horizon.is_some(), "horizon while near the planet".to_string());
        true
    }));

    s.push(Box::new(|w, _| {
        keys(w, &[KeyCode::KeyD], true);
        true
    }));
    s.push(hold_until("strafe right 1 s", &[], 1.0, |_| false));
    s.push(Box::new(|w, c| {
        let f = flight(w);
        let v = f.velocity_dir.unwrap_or(DVec3::ZERO);
        check(c, v.x > 0.5, format!("strafe right: velocity direction {v:?}, want +x"));
        let longest = f.thrust.iter().cloned().fold(0.0, f64::max);
        check(c, f.thrust[0] > 0.0 && f.thrust[0] == longest, format!("strafe right: thrust bars {:?}, want the right one longest", f.thrust));
        keys(w, &[KeyCode::KeyD], false);
        true
    }));
    s.push(hold_until("strafe settle 2 s", &[], 2.0, |_| false));

    s.push(Box::new(|w, _| {
        keys(w, &[KeyCode::KeyW], true);
        true
    }));
    s.push(hold_until("forward 3 s", &[], 3.0, |_| false));
    s.push(Box::new(|w, c| {
        let f = flight(w);
        let v = f.velocity_dir.unwrap_or(DVec3::ZERO);
        check(c, v.z < -0.5, format!("forward: velocity direction {v:?}, want -z"));
        keys(w, &[KeyCode::KeyW], false);
        true
    }));

    for _ in 0..5 {
        s.push(Box::new(|w, _| {
            tap(w, KeyCode::PageDown);
            true
        }));
        s.push(hold_until("PageDown 0.2 s", &[], 0.2, |_| false));
    }
    s.push(Box::new(|_, _| {
        true
    }));

    s.push(hold_until("level 4 s", &[], 4.0, |_| false));
    s.push(Box::new(|_, _| {
        true
    }));

    s.push(Box::new(|w, _| {
        let pl = planet(w);
        let e = ship_e(w);
        let p = w.get::<Position>(e).unwrap().0;
        let up = pl.up(p);
        let top = pl.field.atmosphere_height;
        teleport_ship(w, pl.centre + up * (pl.radius + top + 500.0), crate::ship::basis_for_up(up));
        true
    }));
    s.push(hold_until("above the atmosphere 0.5 s", &[], 0.5, |_| false));
    s.push(Box::new(|w, c| {
        let f = flight(w);
        check(c, f.horizon.is_none(), format!("above the atmosphere: horizon {:?}, want none", f.horizon));
        true
    }));
}
