//! Scenario `camera-g` (#148, #149): the chase camera trails a boost and swings back, the trauma
//! shakes under boost and calms at rest, and F9 (through the bindings) switches all of it off and
//! back on. Driven through `Controls`. The HUD line is not in a headless run; its words are
//! checked in `view::tests`.
use crate::scenario::{above_ground, check, hold_until, land, put_at_seat, ship_vel, sit, tap, wait, with_ship, Step};
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
    s.push(Box::new(|w, c| {
        let f = fx(w);
        check(c, f.enabled, format!("switch on by default: {}", crate::view::camera_fx_text(f.enabled)));
        true
    }));
    s.push(hold_until("boost 2 s", &[KeyCode::KeyW, KeyCode::ShiftLeft], 2.0, |_| false));
    s.push(Box::new(|w, c| {
        let (f, tuning) = (fx(w), w.resource::<crate::tuning::Tuning>().camera.clone());
        check(c, f.trauma > 0.3 && f.trauma <= 1.0, format!("boost 2 s: trauma {:.2} (> 0.3)", f.trauma));
        check(c, f.lag.z > 0.05, format!("boost: the chase camera trails {:.2} m behind the ship", f.lag.z));
        check(c, f.lag.length() <= tuning.lag_max + 1e-9, format!("boost: lag {:.2} m within lag_max {:.1}", f.lag.length(), tuning.lag_max));
        check(c, f.g_fov > 0.0, format!("boost: forward G widens the view by {:.2} deg", f.g_fov));
        true
    }));
    s.push(hold_until("brake to a stop (X)", &[KeyCode::KeyX], 40.0, |w| ship_vel(w).length() < 1.0));
    // F9 off: no shake, lag or G field of view, the HUD says so, and a boost stays shake-free.
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
    // F9 on again, then down to the ground and rest.
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
    s.push(hold_until("descend to 120 m above ground", &[KeyCode::ControlLeft, KeyCode::ShiftLeft], 180.0, |w| above_ground(w) < 120.0));
    s.push(land("land"));
    s.push(wait(3.0));
    s.push(Box::new(|w, c| {
        let (f, landed) = (fx(w), with_ship(w, |s| s.grounded));
        check(c, landed, "landed".into());
        check(c, f.trauma < 0.05 && f.lag.length() < 0.01, format!("rest on the ground: trauma {:.3} (< 0.05), lag {:.4} m (< 0.01)", f.trauma, f.lag.length()));
        true
    }));
}
