//! The chase camera's lag, trauma shake and G field of view, and F9 to switch them off.
//! The HUD line is not in a headless run; its words are checked in `view::tests`.
use crate::scenario::{above_ground, check, hold_until, land, planet, put_at_seat, ship_e, ship_frame_of, ship_vel, sit, tap, teleport_ship, wait, with_ship, Step};
use crate::ship::basis_for_up;
use avian3d::prelude::Position;
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use flight_core::camera::CameraFx;

fn fx(w: &World) -> CameraFx {
    w.resource::<crate::ship::CameraEffects>().0
}

fn no_effects(f: &CameraFx) -> bool {
    f.trauma == 0.0 && f.shake_offset == DVec3::ZERO && f.shake_angle == DVec2::ZERO && f.lag == DVec3::ZERO && f.g_fov == 0.0
}

pub fn camera_g_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(hold_until("climb to 300 m above ground", &[KeyCode::Space], 90.0, |w| above_ground(w) > 300.0));
    s.push(wait(3.0));
    s.push(hold_until("boost 2 s", &[KeyCode::KeyW, KeyCode::ShiftLeft], 2.0, |_| false));
    s.push(Box::new(|w, c| {
        let (f, tuning) = (fx(w), w.resource::<crate::tuning::Tuning>().camera.clone());
        check(c, f.lag.length() <= tuning.lag_max + 1e-9, format!("boost: lag {:.2} m within lag_max {:.1}", f.lag.length(), tuning.lag_max));
        true
    }));
    s.push(hold_until("brake to a stop (X)", &[KeyCode::KeyX], 40.0, |w| ship_vel(w).length() < 1.0));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F9);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let f = fx(w);
        check(c, !f.enabled, format!("F9 switches off: {}", crate::view::camera_fx_text(f.enabled)));
        check(c, no_effects(&f), "switch off: no shake, lag or G field of view".into());
        c.v.insert("v0", ship_vel(w).length());
        true
    }));
    s.push(hold_until("boost 2 s, switch off", &[KeyCode::KeyW, KeyCode::ShiftLeft], 2.0, |_| false));
    s.push(Box::new(|w, c| {
        let (f, v, v0) = (fx(w), ship_vel(w).length(), c.v["v0"]);
        check(c, v > v0 + 3.0, format!("the ship boosted ({v0:.1} -> {v:.1} m/s), so the check is not empty"));
        check(c, no_effects(&f), "switch off: the boost gives no shake, lag or G field of view".into());
        true
    }));
    s.push(hold_until("brake to a stop (X)", &[KeyCode::KeyX], 40.0, |w| ship_vel(w).length() < 1.0));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F9);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let f = fx(w);
        check(c, f.enabled, format!("F9 on again: {}", crate::view::camera_fx_text(f.enabled)));
        true
    }));
    s.push(wait(3.0));
    s.push(Box::new(|w, c| {
        c.v.insert("hover_lag", 0.0);
        c.v.insert("hover_g", 0.0);
        check(c, with_ship(w, |s| s.sc.modes.grav_comp), "hover: assist on, no input".into());
        true
    }));
    s.push(Box::new(|w, c| {
        let f = fx(w);
        let (lag, g) = (c.v["hover_lag"].max(f.lag.length()), c.v["hover_g"].max(f.g_fov));
        c.v.insert("hover_lag", lag);
        c.v.insert("hover_g", g);
        if c.t < 2.0 {
            return false;
        }
        check(c, above_ground(w) > 100.0, format!("hover at {:.0} m above ground", above_ground(w)));
        true
    }));
    s.push(hold_until("descend to 120 m above ground", &[KeyCode::ControlLeft, KeyCode::ShiftLeft], 180.0, |w| above_ground(w) < 120.0));
    // Brake to rest first: the descent at 66 m/s still moves sideways at touchdown (SC has no
    // landing gear), which is not what this scenario checks. Then a test hook levels the
    // ship at its spot: the SC's down input follows the hull, so a tilted hull slides on the way
// down (the axis model's fix does not carry over to SC).
    s.push(hold_until("firm brake before the landing", &[KeyCode::KeyX], 20.0, |w| ship_vel(w).length() < 0.5));
    s.push(Box::new(|w, _| {
        let e = ship_e(w);
        let (p, up) = (w.get::<Position>(e).unwrap().0, planet(w).up(ship_frame_of(w).origin));
        teleport_ship(w, p, basis_for_up(up));
        true
    }));
    s.push(land("land"));
    s.push(wait(3.0));
    s.push(Box::new(|w, c| {
        let agl = above_ground(w);
        let grounded = with_ship(w, |s| s.grounded);
        check(c, grounded, format!("landed: {agl:.2} m above ground (contact flag {grounded})"));
        true
    }));
}
