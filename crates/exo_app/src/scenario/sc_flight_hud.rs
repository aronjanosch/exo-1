//! Scenario `sc-flight-hud` (#200): the flight HUD's readout through `Controls`, SC model. Sit,
//! lift and hover; D strafes right (the velocity points along +x, the right thrust bar is the
//! longest); W flies forward (the tape fills, the velocity points forward); Page Down x5 sets the
//! limiter to 50 % (its mark on the tape); released, the horizon is level.
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
        let cruise = f.speed_tape.cruise;
        check(c, (cruise - 150.0 / 337.5).abs() < 0.01, format!("SCM cruise mark at {cruise:.3}, want 0.444"));
        check(c, f.speed_tape.limiter.is_none(), "limiter mark off at full speed".to_string());
        true
    }));

    // Lift and hover on the ground's horizon.
    s.push(hold_until("lift 2 s", &[KeyCode::Space], 2.0, |_| false));
    s.push(hold_until("settle 2 s", &[], 2.0, |_| false));
    s.push(Box::new(|w, c| {
        let f = flight(w);
        check(c, f.horizon.is_some(), "horizon while near the planet".to_string());
        check(c, f.g_bar.mark > 0.0 && f.g_bar.mark < 1.0, format!("G-safe mark {:.2}", f.g_bar.mark));
        true
    }));

    // D: strafe right 1 s. The velocity points along +x, the right bar is the longest.
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

    // W: forward 3 s. The tape fills, the velocity points forward (-z in ship space).
    s.push(Box::new(|w, c| {
        c.v.insert("fill0", flight(w).speed_tape.fill);
        keys(w, &[KeyCode::KeyW], true);
        true
    }));
    s.push(hold_until("forward 3 s", &[], 3.0, |_| false));
    s.push(Box::new(|w, c| {
        let f = flight(w);
        let (before, now) = (c.v["fill0"], f.speed_tape.fill);
        check(c, now > before + 0.05, format!("tape fill {before:.3} -> {now:.3} after 3 s of W"));
        let v = f.velocity_dir.unwrap_or(DVec3::ZERO);
        check(c, v.z < -0.5, format!("forward: velocity direction {v:?}, want -z"));
        keys(w, &[KeyCode::KeyW], false);
        true
    }));

    // Page Down x5: the limiter at 50 %, its mark on the tape at cruise x 0.5 over the boost cap.
    for _ in 0..5 {
        s.push(Box::new(|w, _| {
            tap(w, KeyCode::PageDown);
            true
        }));
        s.push(hold_until("PageDown 0.2 s", &[], 0.2, |_| false));
    }
    s.push(Box::new(|w, c| {
        let f = flight(w);
        let want = 0.5 * 150.0 / 337.5;
        let got = f.speed_tape.limiter.unwrap_or(f64::NAN);
        check(c, (got - want).abs() < 0.005, format!("limiter mark {got:.3}, want {want:.3} (50 %)"));
        let r = w.resource::<HudReadout>();
        check(c, r.badges.iter().any(|b| b.key == "LIMIT" && b.label == "LIMIT 50 %"), format!("LIMIT 50 % badge: {:?}", r.badges.iter().map(|b| b.label.clone()).collect::<Vec<_>>()));
        true
    }));

    // Released: the ship is level, the horizon's roll is within 2 deg.
    s.push(hold_until("level 4 s", &[], 4.0, |_| false));
    s.push(Box::new(|w, c| {
        let f = flight(w);
        let (pitch, roll) = f.horizon.unwrap_or((f64::NAN, f64::NAN));
        check(c, roll.abs() < 2.0, format!("horizon: pitch {pitch:+.1} deg, roll {roll:+.2} deg, want roll within 2 deg"));
        true
    }));

    // Above the atmosphere the horizon goes (a test hook lifts the ship out of it).
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
